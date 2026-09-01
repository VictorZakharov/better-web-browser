use super::boxes::{self, BoxView};
use super::init::{Metadata, SampleDefaults};
use super::{VideoSample, add_signed, scale, scale_signed};
use crate::limits::MAX_MEDIA_DECODED_SAMPLES;

pub(super) fn parse(
    source: &[u8],
    moof: BoxView<'_>,
    metadata: &Metadata,
    output: &mut Vec<VideoSample>,
) -> Result<(), String> {
    for traf in boxes::parse(moof.payload, moof.start + moof.header)?
        .into_iter()
        .filter(|entry| entry.kind == *b"traf")
    {
        let children = boxes::parse(traf.payload, traf.start + traf.header)?;
        let Some(tfhd) = children.iter().find(|entry| entry.kind == *b"tfhd") else {
            continue;
        };
        let header = parse_tfhd(tfhd.payload, metadata.defaults)?;
        if header.track_id != metadata.track_id {
            continue;
        }
        let tfdt = children
            .iter()
            .find(|entry| entry.kind == *b"tfdt")
            .ok_or_else(|| "H.264 fragment has no decode time".to_string())?;
        let mut decode_time = parse_tfdt(tfdt.payload)?;
        let mut data_cursor = None;
        for trun in children.iter().filter(|entry| entry.kind == *b"trun") {
            let run = parse_trun(trun.payload, header.defaults)?;
            if let Some(offset) = run.data_offset {
                let base = header.base_data_offset.unwrap_or(moof.start as u64);
                data_cursor = Some(add_signed(base, offset)?);
            }
            let mut cursor = data_cursor
                .ok_or_else(|| "H.264 fragment has no sample data offset".to_string())?;
            for sample in run.samples {
                let end = cursor
                    .checked_add(sample.size as u64)
                    .ok_or_else(|| "H.264 sample offset overflowed".to_string())?;
                let start = usize::try_from(cursor)
                    .map_err(|_| "H.264 sample start is not representable")?;
                let end_index =
                    usize::try_from(end).map_err(|_| "H.264 sample end is not representable")?;
                if end_index > source.len() || start >= end_index {
                    return Err("H.264 sample points outside fragmented MP4".into());
                }
                let presentation = add_signed(decode_time, sample.composition_offset)?;
                output.push(VideoSample {
                    bytes: source[start..end_index].to_vec(),
                    timestamp_100ns: scale_signed(presentation, metadata.timescale)?,
                    duration_100ns: scale(sample.duration as u64, metadata.timescale)?,
                    key_frame: sample.flags & 0x0001_0000 == 0,
                });
                decode_time = decode_time
                    .checked_add(sample.duration as u64)
                    .ok_or_else(|| "H.264 decode time overflowed".to_string())?;
                cursor = end;
            }
            data_cursor = Some(cursor);
        }
    }
    Ok(())
}

struct TrackFragmentHeader {
    track_id: u32,
    base_data_offset: Option<u64>,
    defaults: SampleDefaults,
}

fn parse_tfhd(payload: &[u8], mut defaults: SampleDefaults) -> Result<TrackFragmentHeader, String> {
    if payload.len() < 8 {
        return Err("track-fragment header is truncated".into());
    }
    let flags = boxes::flags(payload)?;
    let track_id = boxes::u32_at(payload, 4)?;
    let mut offset = 8;
    let base_data_offset = if flags & 0x000001 != 0 {
        let value = boxes::u64_at(payload, offset)?;
        offset += 8;
        Some(value)
    } else {
        None
    };
    if flags & 0x000002 != 0 {
        offset += 4;
    }
    if flags & 0x000008 != 0 {
        defaults.duration = boxes::u32_at(payload, offset)?;
        offset += 4;
    }
    if flags & 0x000010 != 0 {
        defaults.size = boxes::u32_at(payload, offset)?;
        offset += 4;
    }
    if flags & 0x000020 != 0 {
        defaults.flags = boxes::u32_at(payload, offset)?;
    }
    Ok(TrackFragmentHeader {
        track_id,
        base_data_offset,
        defaults,
    })
}

struct TrackRun {
    data_offset: Option<i64>,
    samples: Vec<RunSample>,
}

struct RunSample {
    duration: u32,
    size: u32,
    flags: u32,
    composition_offset: i64,
}

fn parse_trun(payload: &[u8], defaults: SampleDefaults) -> Result<TrackRun, String> {
    if payload.len() < 8 {
        return Err("track run is truncated".into());
    }
    let version = payload[0];
    let flags = boxes::flags(payload)?;
    let count = boxes::u32_at(payload, 4)? as usize;
    if count == 0 || count > MAX_MEDIA_DECODED_SAMPLES {
        return Err("track-run sample count exceeds worker limits".into());
    }
    let mut offset = 8;
    let data_offset = if flags & 0x000001 != 0 {
        let value = boxes::u32_at(payload, offset)? as i32 as i64;
        offset += 4;
        Some(value)
    } else {
        None
    };
    let first_flags = if flags & 0x000004 != 0 {
        let value = boxes::u32_at(payload, offset)?;
        offset += 4;
        Some(value)
    } else {
        None
    };
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let duration = optional_u32(
            payload,
            &mut offset,
            flags & 0x000100 != 0,
            defaults.duration,
        )?;
        let size = optional_u32(payload, &mut offset, flags & 0x000200 != 0, defaults.size)?;
        let sample_flags = optional_u32(
            payload,
            &mut offset,
            flags & 0x000400 != 0,
            if index == 0 {
                first_flags.unwrap_or(defaults.flags)
            } else {
                defaults.flags
            },
        )?;
        let composition_offset = if flags & 0x000800 != 0 {
            let raw = boxes::u32_at(payload, offset)?;
            offset += 4;
            if version == 0 {
                i64::from(raw)
            } else {
                i64::from(raw as i32)
            }
        } else {
            0
        };
        if duration == 0 || size == 0 {
            return Err("track run has a zero-sized or zero-duration sample".into());
        }
        samples.push(RunSample {
            duration,
            size,
            flags: sample_flags,
            composition_offset,
        });
    }
    Ok(TrackRun {
        data_offset,
        samples,
    })
}

fn parse_tfdt(payload: &[u8]) -> Result<u64, String> {
    if payload.is_empty() {
        return Err("fragment decode time is truncated".into());
    }
    if payload[0] == 1 {
        boxes::u64_at(payload, 4)
    } else {
        Ok(u64::from(boxes::u32_at(payload, 4)?))
    }
}

fn optional_u32(
    bytes: &[u8],
    offset: &mut usize,
    present: bool,
    default: u32,
) -> Result<u32, String> {
    if !present {
        return Ok(default);
    }
    let value = boxes::u32_at(bytes, *offset)?;
    *offset += 4;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_track_run_fields() {
        let payload = [
            0, 0, 0x0f, 0x01, // data offset plus every per-sample field
            0, 0, 0, 1, // one sample
            0, 0, 0, 32, // data offset
            0, 0, 3, 0xe8, // duration
            0, 0, 0, 5, // size
            0, 0, 0, 0, // flags
            0, 0, 0, 2, // composition offset
        ];
        let run = parse_trun(&payload, SampleDefaults::default()).unwrap();
        assert_eq!(run.data_offset, Some(32));
        assert_eq!(run.samples[0].duration, 1_000);
        assert_eq!(run.samples[0].size, 5);
        assert_eq!(run.samples[0].composition_offset, 2);
    }
}
