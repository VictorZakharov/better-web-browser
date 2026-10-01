//! Preserve the timing fields currently omitted by the upstream packet API.
//! Lacing itself remains Symphonia's responsibility; the fixed prefix only
//! supplies a frame count, track number, and block timestamp for accounting.

use super::{elements::*, header};
use crate::encoded_audio::{Budget, webm};

pub(super) struct Metadata {
    pub(super) head: header::Header,
    pub(super) number: u32,
    pub(super) scale_ns: u64,
    pub(super) blocks: Vec<Block>,
    pub(super) unknown_segment: bool,
    pub(super) unknown_cluster: bool,
}

pub(super) struct Block {
    pub(super) time_ticks: i128,
    pub(super) frames: usize,
    pub(super) padding_ns: i64,
    laced: bool,
}

struct Track {
    head: header::Header,
    number: u32,
    lacing: bool,
}

pub(super) fn codec_id(bytes: &[u8]) -> Result<&[u8], String> {
    let mut result = None;
    let mut budget = Budget::new(None);
    children(bytes, &mut budget, |element, budget| {
        if element.id == SEGMENT {
            children(element.data, budget, |element, budget| {
                if element.id == TRACKS {
                    children(element.data, budget, |element, budget| {
                        if element.id == TRACK {
                            children(element.data, budget, |element, _| {
                                if element.id == 0x86 {
                                    unique(&mut result, element.data, "CodecID")?;
                                }
                                Ok(())
                            })?;
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    result.ok_or_else(|| "WebM has no CodecID".into())
}

pub(super) fn inspect(bytes: &[u8], budget: &mut Budget<'_>) -> Result<Metadata, String> {
    webm::validate_profile(bytes, budget, true)?;
    let mut track = None;
    let mut scale = None;
    let mut blocks = Vec::new();
    let mut unknown_segment = false;
    let mut unknown_cluster = false;
    children(bytes, budget, |element, budget| {
        if element.id != SEGMENT {
            return Ok(());
        }
        unknown_segment = element.unknown_size;
        children(element.data, budget, |element, budget| {
            match element.id {
                INFO => children(element.data, budget, |element, _| {
                    if element.id == 0x002a_d7b1 {
                        unique(&mut scale, webm::uint(element.data)?, "TimestampScale")?;
                    }
                    Ok(())
                })?,
                TRACKS => children(element.data, budget, |element, budget| {
                    if element.id == TRACK {
                        unique(&mut track, read_track(element.data, budget)?, "TrackEntry")?;
                    }
                    Ok(())
                })?,
                CLUSTER => {
                    unknown_cluster |= element.unknown_size;
                    read_cluster(element.data, budget, &mut blocks)?;
                }
                _ => (),
            }
            Ok(())
        })
    })?;
    let Track {
        head,
        number,
        lacing,
    } = track.ok_or("WebM Opus has no complete track")?;
    // FlagLacing forbids the flags themselves, including a one-frame lace.
    // Demuxing a legal packet does not authorize contradictory track metadata.
    if !lacing && blocks.iter().any(|block| block.laced) {
        return Err("WebM Opus Block contradicts disabled FlagLacing".into());
    }
    let scale_ns = scale.unwrap_or(1_000_000);
    if scale_ns == 0 || scale_ns > 1_000_000_000 {
        return Err("WebM Opus TimestampScale is outside the admitted timeline".into());
    }
    Ok(Metadata {
        head,
        number,
        scale_ns,
        blocks,
        unknown_segment,
        unknown_cluster,
    })
}

fn read_track(bytes: &[u8], budget: &mut Budget<'_>) -> Result<Track, String> {
    let (mut head, mut number, mut delay, mut preroll) = (None, None, None, None);
    let (mut channels, mut sampling) = (None, None);
    let (mut lacing, mut scale, mut offset, mut output, mut emphasis) =
        (None, None, None, None, None);
    children(bytes, budget, |element, budget| {
        match element.id {
            0x63a2 => unique(&mut head, header::read(element.data)?, "CodecPrivate")?,
            0xd7 => unique(&mut number, webm::uint(element.data)?, "TrackNumber")?,
            0x56aa => unique(&mut delay, webm::uint(element.data)?, "CodecDelay")?,
            0x56bb => unique(&mut preroll, webm::uint(element.data)?, "SeekPreRoll")?,
            0x9c => unique(
                &mut lacing,
                unsigned_default(element.data, 1)?,
                "FlagLacing",
            )?,
            0x0023_314f => unique(
                &mut scale,
                if element.data.is_empty() {
                    1.0
                } else {
                    float(element.data)?
                },
                "TrackTimestampScale",
            )?,
            0x537f => unique(
                &mut offset,
                if element.data.is_empty() {
                    0
                } else {
                    signed(element.data)?
                },
                "TrackOffset",
            )?,
            0xe1 => children(element.data, budget, |element, _| {
                match element.id {
                    0x9f => unique(&mut channels, webm::uint(element.data)?, "Channels")?,
                    0xb5 => unique(&mut sampling, float(element.data)?, "SamplingFrequency")?,
                    0x78b5 => unique(&mut output, float(element.data)?, "OutputSamplingFrequency")?,
                    0x52f1 => unique(
                        &mut emphasis,
                        unsigned_default(element.data, 0)?,
                        "Emphasis",
                    )?,
                    _ => (),
                }
                Ok(())
            })?,
            _ => (),
        }
        Ok(())
    })?;
    if lacing.is_some_and(|value| value > 1) {
        return Err("WebM Opus FlagLacing must be zero or one".into());
    }
    if scale.is_some_and(|value| value != 1.0) {
        return Err("WebM Opus scaled TrackTimestampScale is unsupported".into());
    }
    // These transforms cannot be ignored without altering presentation. The
    // bounded profile supports their neutral defaults, not silent normalization.
    if offset.is_some_and(|value| value != 0) {
        return Err("WebM Opus nonzero TrackOffset is unsupported".into());
    }
    if output.is_some_and(|value| value != 48_000.0) {
        return Err("WebM Opus output sampling frequency is unsupported".into());
    }
    if emphasis.is_some_and(|value| value != 0) {
        return Err("WebM Opus audio Emphasis is unsupported".into());
    }
    let head = head.ok_or("WebM Opus CodecPrivate is missing")?;
    let number = number
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n != 0)
        .ok_or("WebM Opus TrackNumber is missing or invalid")?;
    if channels.unwrap_or(1) != u64::from(head.channels) {
        return Err("WebM Opus Channels disagrees with OpusHead".into());
    }
    let sampling = sampling.unwrap_or(8_000.0);
    if !sampling.is_finite() || sampling <= 0.0 || sampling != f64::from(head.input_rate) {
        return Err("WebM Opus SamplingFrequency disagrees with OpusHead".into());
    }
    // Matroska A_OPUS mapping mandates CodecDelay derived from 48 kHz pre-skip.
    // Integer nanoseconds may be rounded either way by conforming writers.
    let expected = u64::from(head.pre_skip) * 1_000_000_000 / 48_000;
    if delay.is_none_or(|value| value.abs_diff(expected) > 1) {
        return Err("WebM Opus CodecDelay is missing or disagrees with pre-skip".into());
    }
    if preroll.is_some_and(|value| value > 1_000_000_000) {
        return Err("WebM Opus SeekPreRoll exceeds the admitted bound".into());
    }
    // Exact seeks replay from the beginning, thereby retaining all codec history
    // (including the recommended 80ms pre-roll) instead of skipping raw packets.
    Ok(Track {
        head,
        number,
        lacing: lacing.unwrap_or(1) != 0,
    })
}

fn unsigned_default(bytes: &[u8], default: u64) -> Result<u64, String> {
    if bytes.is_empty() {
        Ok(default)
    } else {
        webm::uint(bytes)
    }
}

fn read_cluster(
    bytes: &[u8],
    budget: &mut Budget<'_>,
    blocks: &mut Vec<Block>,
) -> Result<(), String> {
    let mut timestamp = None;
    children(bytes, budget, |element, budget| {
        match element.id {
            0xe7 => unique(
                &mut timestamp,
                webm::uint(element.data)?,
                "Cluster Timestamp",
            )?,
            0xa3 => add_block(element.data, timestamp, 0, blocks)?,
            0xa0 => {
                let (mut block, mut padding) = (None, None);
                children(element.data, budget, |element, _| {
                    match element.id {
                        0xa1 => unique(&mut block, element.data, "Block")?,
                        0x75a2 => unique(&mut padding, signed(element.data)?, "DiscardPadding")?,
                        // Reconfiguration/encryption cannot mutate admitted decoder state.
                        0xa4 | 0x75a1 => {
                            return Err("WebM Opus Block additions/state are unsupported".into());
                        }
                        _ => (),
                    }
                    Ok(())
                })?;
                add_block(
                    block.ok_or("WebM Opus BlockGroup has no Block")?,
                    timestamp,
                    padding.unwrap_or(0),
                    blocks,
                )?;
            }
            _ => (),
        }
        Ok(())
    })
}

fn add_block(
    bytes: &[u8],
    timestamp: Option<u64>,
    padding_ns: i64,
    blocks: &mut Vec<Block>,
) -> Result<(), String> {
    if blocks.len() >= 16_384 {
        return Err("WebM Opus block count exceeds admission limit".into());
    }
    let (_, width) = webm::vint(bytes, false)?;
    let prefix = bytes
        .get(width..width + 3)
        .ok_or("WebM Opus Block prefix is truncated")?;
    let relative = i16::from_be_bytes([prefix[0], prefix[1]]);
    let frames = if prefix[2] & 6 == 0 {
        1
    } else {
        usize::from(
            *bytes
                .get(width + 3)
                .ok_or("WebM Opus lace count is missing")?,
        ) + 1
    };
    blocks.push(Block {
        time_ticks: i128::from(timestamp.ok_or("WebM Opus Cluster has no Timestamp")?)
            + i128::from(relative),
        frames,
        padding_ns,
        laced: prefix[2] & 6 != 0,
    });
    Ok(())
}
