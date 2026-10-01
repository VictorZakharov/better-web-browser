//! Symphonia owns packet extraction and all three Matroska lacing modes.

use super::{metadata::Metadata, timeline::Timeline};
use crate::opus_audio::Limits;
use std::io::Cursor;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Instant;
use symphonia::core::codecs::{CodecParameters, audio::well_known::CODEC_ID_OPUS};
use symphonia::core::common::Limit;
use symphonia::core::formats::well_known::FORMAT_ID_MKV;
use symphonia::core::formats::{FormatOptions, TrackType, probe::Hint};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub(super) struct Packet {
    pub(super) data: Box<[u8]>,
    pub(super) frames: usize,
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) gap: u64,
}

pub(super) struct Admitted {
    pub(super) packets: Vec<Packet>,
    pub(super) frames: u64,
}

pub(super) fn check(cancelled: Option<&AtomicBool>, deadline: Instant) -> Result<(), String> {
    if cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err("WebM Opus decoding was cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("WebM Opus decoding exceeded its deadline".into());
    }
    Ok(())
}

pub(super) fn admit(
    source: Arc<[u8]>,
    metadata: &Metadata,
    limits: Limits,
    cancelled: Option<&AtomicBool>,
    deadline: Instant,
) -> Result<Admitted, String> {
    check(cancelled, deadline)?;
    let mut hint = Hint::new();
    hint.with_extension("webm");
    let media = MediaSourceStream::new(Box::new(Cursor::new(source)), Default::default());
    let options = MetadataOptions::default()
        .limit_tag_bytes(Limit::Maximum(64 * 1024))
        .limit_visual_bytes(Limit::Maximum(0));
    let mut reader = symphonia::default::get_probe()
        .probe(&hint, media, FormatOptions::default(), options)
        .map_err(|error| format!("open WebM Opus demuxer: {error}"))?;
    if reader.format_info().format != FORMAT_ID_MKV || reader.tracks().len() != 1 {
        return Err("WebM Opus requires exactly one Matroska audio track".into());
    }
    let track = reader
        .first_track(TrackType::Audio)
        .ok_or("WebM Opus audio track is missing")?;
    if track.id != metadata.number
        || !matches!(track.codec_params.as_ref(),
        Some(CodecParameters::Audio(params)) if params.codec == CODEC_ID_OPUS)
    {
        return Err("WebM Opus track disagrees with admitted metadata".into());
    }
    let mut packets = Vec::new();
    let mut timeline = Timeline::default();
    let mut raw_total = 0_u64;
    let mut encoded_total = 0_usize;
    for block in &metadata.blocks {
        let first = packets.len();
        let mut raw_frames = 0_u64;
        for _ in 0..block.frames {
            check(cancelled, deadline)?;
            if packets.len() >= limits.max_packets.min(16_384) {
                return Err("WebM Opus exceeds its packet count limit".into());
            }
            let packet = reader
                .next_packet()
                .map_err(|error| format!("demux WebM Opus block: {error}"))?
                .ok_or("WebM Opus block packet count is incomplete")?;
            if packet.track_id != metadata.number
                || packet.data.is_empty()
                || packet.data.len() > 61_440
            {
                return Err(
                    "WebM Opus packet is empty, oversized, or belongs to another track".into(),
                );
            }
            encoded_total = encoded_total
                .checked_add(packet.data.len())
                .filter(|n| *n <= crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES)
                .ok_or("WebM Opus packet storage exceeds source limit")?;
            opus::packet::parse(&packet.data)
                .map_err(|error| format!("invalid WebM Opus packet: {error}"))?;
            let frames = opus::packet::get_nb_samples(&packet.data, 48_000)
                .map_err(|error| format!("invalid WebM Opus duration: {error}"))?;
            if frames == 0 || frames > 5_760 {
                return Err("WebM Opus packet exceeds 120 ms".into());
            }
            raw_frames = raw_frames
                .checked_add(frames as u64)
                .ok_or("WebM Opus raw duration overflows")?;
            packets.push(Packet {
                data: packet.data,
                frames,
                left: 0,
                right: frames,
                gap: 0,
            });
        }
        raw_total = raw_total
            .checked_add(raw_frames)
            .ok_or("WebM Opus raw duration overflows")?;
        bound(raw_total, metadata.head.channels, limits)?;
        let window =
            timeline.block(block, metadata.scale_ns, metadata.head.pre_skip, raw_frames)?;
        let mut offset = 0_u64;
        for (index, packet) in packets[first..].iter_mut().enumerate() {
            let end = offset + packet.frames as u64;
            packet.left = window.left.saturating_sub(offset).min(packet.frames as u64) as usize;
            packet.right = window
                .right
                .min(end)
                .saturating_sub(offset)
                .max(packet.left as u64) as usize;
            if index == 0 {
                packet.gap = window.gap;
            }
            offset = end;
        }
        bound(timeline.presented, metadata.head.channels, limits)?;
    }
    check(cancelled, deadline)?;
    match reader.next_packet() {
        Ok(None) => (),
        // Upstream's unknown Segment/Cluster scanner requests another element at EOF.
        // Accept this only after bounded envelope validation and consumption of
        // every declared Block/lace, never during packet extraction.
        Err(symphonia::core::errors::Error::IoError(error))
            if (metadata.unknown_segment || metadata.unknown_cluster)
                && error.kind() == std::io::ErrorKind::UnexpectedEof => {}
        Ok(Some(_)) => return Err("WebM Opus has unaccounted packets".into()),
        Err(error) => return Err(format!("finish WebM Opus demux: {error}")),
    }
    if timeline.presented == 0 {
        return Err("WebM Opus has no presentation audio".into());
    }
    Ok(Admitted {
        packets,
        frames: timeline.presented,
    })
}

fn bound(frames: u64, channels: u16, limits: Limits) -> Result<(), String> {
    if frames > limits.max_duration_frames.min(48_000 * 3_600)
        || frames
            .checked_mul(u64::from(channels))
            .and_then(|n| n.checked_mul(4))
            .is_none_or(|n| n > limits.max_decoded_bytes as u64)
    {
        return Err("WebM Opus PCM exceeds its byte or duration limit".into());
    }
    Ok(())
}
