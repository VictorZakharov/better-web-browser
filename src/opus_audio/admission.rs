use super::timeline::Timeline;
use super::{
    Budget, Limits, MAX_DURATION_FRAMES, MAX_PACKET_BYTES, MAX_PACKET_FRAMES, MAX_PACKETS,
};
use crate::encoded_audio::ogg_envelope;
use ogg::reading::PacketReader;
use std::io::Cursor;
use symphonia::core::io::BufReader;
use symphonia_common::xiph::audio::opus::OpusHead;

const MAX_HEADER_BYTES: usize = 256 * 1024;

pub(super) struct Admitted {
    pub(super) channels: u16,
    pub(super) gain: i16,
    pub(super) pre_skip: u16,
    pub(super) raw_frames: u64,
    pub(super) end: u64,
    pub(super) durations: Vec<u16>,
}

pub(super) fn validate(
    bytes: &[u8],
    limits: Limits,
    budget: &mut Budget<'_>,
) -> Result<Admitted, String> {
    let envelope = ogg_envelope::inspect(
        bytes,
        |index| {
            if index < 2 {
                MAX_HEADER_BYTES
            } else {
                MAX_PACKET_BYTES
            }
        },
        || budget.step(),
    )?;
    let audio_page = header_pages(&envelope.pages)?;
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let identification = reader
        .read_packet()
        .map_err(|error| format!("read Ogg Opus identification: {error}"))?
        .ok_or("Ogg Opus identification is missing")?;
    let data = &identification.data;
    if data.get(18) != Some(&0) || !data.get(9).is_some_and(|count| (1..=2).contains(count)) {
        return Err("Ogg Opus supports only mapping family 0 mono/stereo".into());
    }
    let head = OpusHead::read(&mut BufReader::new(data), 15)
        .map_err(|error| format!("invalid Ogg Opus identification: {error}"))?;
    // Compatible future minor versions may extend the header (RFC 7845 5.1).
    if head.version <= 1 && data.len() != 19 {
        return Err("Ogg Opus family 0 identification has unexpected mapping bytes".into());
    }
    let channels = u16::from(data[9]);
    let comments = reader
        .read_packet()
        .map_err(|error| format!("read Ogg Opus comments: {error}"))?
        .ok_or("Ogg Opus comments are missing")?;
    if comments.data.get(..8) != Some(b"OpusTags") {
        return Err("Ogg Opus comment signature is invalid".into());
    }
    // Unknown fields and trailing comment padding are bounded but not decoded.
    crate::ogg_vorbis_headers::preflight_comment_fields(&comments.data[8..], |_| budget.step())?;
    let mut timeline = Timeline::default();
    let mut durations = Vec::new();
    for page in &envelope.pages[audio_page..] {
        budget.step()?;
        if page.completed_packets == 0 {
            if page.granule != u64::MAX || page.eos {
                return Err("Ogg Opus incomplete audio page must have an unset granule".into());
            }
            continue;
        }
        let mut samples = 0_u64;
        for _ in 0..page.completed_packets {
            budget.step()?;
            if durations.len() >= limits.max_packets.min(MAX_PACKETS) {
                return Err("Ogg Opus exceeds the bounded audio packet count".into());
            }
            let packet = reader
                .read_packet()
                .map_err(|error| format!("read Ogg Opus audio admission: {error}"))?
                .ok_or("Ogg Opus audio is incomplete")?;
            if packet.stream_serial() != envelope.serial || packet.data.is_empty() {
                return Err("Ogg Opus audio packet is empty or multiplexed".into());
            }
            // libopus parses the full framing, not just TOC duration bits. Empty
            // codec frames inside a nonempty packet are valid; PLC input is not.
            opus::packet::parse(&packet.data)
                .map_err(|error| format!("invalid Ogg Opus packet framing: {error}"))?;
            let frames = opus::packet::get_nb_samples(&packet.data, super::SAMPLE_RATE)
                .map_err(|error| format!("invalid Ogg Opus packet duration: {error}"))?;
            if frames == 0 || frames > MAX_PACKET_FRAMES {
                return Err("Ogg Opus packet exceeds the 120 ms duration limit".into());
            }
            durations.push(frames as u16);
            samples = samples
                .checked_add(frames as u64)
                .ok_or("Ogg Opus page duration overflows")?;
        }
        timeline.page(samples, page.granule, page.eos)?;
        bound_pcm(timeline.raw_frames, channels, limits)?;
    }
    if reader
        .read_packet()
        .map_err(|error| format!("finish Ogg Opus admission: {error}"))?
        .is_some()
    {
        return Err("Ogg Opus contains unaccounted audio packets".into());
    }
    let end = timeline.finish(head.pre_skip)?;
    bound_pcm(end - u64::from(head.pre_skip), channels, limits)?;
    Ok(Admitted {
        channels,
        gain: head.gain,
        pre_skip: head.pre_skip,
        raw_frames: timeline.raw_frames,
        end,
        durations,
    })
}

fn header_pages(pages: &[ogg_envelope::Page]) -> Result<usize, String> {
    let first_page = &pages[0];
    if first_page.completed_packets != 1
        || first_page.continued
        || first_page.eos
        || first_page.granule != 0
    {
        return Err("Ogg Opus identification must occupy its own complete BOS page".into());
    }
    let mut audio_page = 1;
    loop {
        let page = pages
            .get(audio_page)
            .ok_or("Ogg Opus comments are incomplete")?;
        if audio_page == 1 && page.segments == 0 {
            return Err("Ogg Opus comments must begin on the second physical page".into());
        }
        audio_page += 1;
        if page.eos
            || (page.completed_packets == 0 && page.granule != u64::MAX)
            || (page.completed_packets != 0
                && (page.completed_packets != 1 || page.continued || page.granule != 0))
        {
            return Err("Ogg Opus comments must end on their own non-audio page".into());
        }
        if page.completed_packets == 1 {
            break;
        }
    }
    Ok(audio_page)
}

fn bound_pcm(frames: u64, channels: u16, limits: Limits) -> Result<(), String> {
    // Both raw predictive history and presented PCM have independent ceilings;
    // pre-skip or EOS trimming cannot hide an oversized decode workload.
    let bytes = frames
        .checked_mul(u64::from(channels))
        .and_then(|samples| samples.checked_mul(4))
        .ok_or("Ogg Opus decoded PCM size overflows")?;
    if frames > limits.max_duration_frames.min(MAX_DURATION_FRAMES)
        || bytes
            > limits
                .max_decoded_bytes
                .min(crate::limits::MAX_MEDIA_DECODED_SOURCE_BYTES as usize) as u64
    {
        return Err("Ogg Opus decoded PCM exceeds the configured raw/presentation limit".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_cannot_skip_the_second_physical_page() {
        let page = |granule, segments, completed_packets| ogg_envelope::Page {
            granule,
            segments,
            completed_packets,
            continued: false,
            eos: false,
        };
        let pages = [page(0, 1, 1), page(u64::MAX, 0, 0), page(0, 1, 1)];
        assert!(
            header_pages(&pages)
                .unwrap_err()
                .contains("second physical page")
        );
        let pages = [page(0, 1, 1), page(u64::MAX, 255, 0), page(0, 1, 1)];
        assert_eq!(header_pages(&pages).unwrap(), 3);
    }
}
