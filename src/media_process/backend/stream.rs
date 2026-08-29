use crate::limits::{
    MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DECODED_SOURCE_BYTES, MAX_MEDIA_DURATION_100NS,
};
use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
};

pub(super) struct StreamSummary {
    pub(super) samples: u32,
    pub(super) bytes: u64,
    pub(super) end_100ns: u64,
    pub(super) first_timestamp: Option<i64>,
    pub(super) last_timestamp: Option<i64>,
}

pub(super) fn read_stream(
    reader: &IMFSourceReader,
    stream: u32,
    name: &str,
    max_sample_bytes: u64,
) -> Result<StreamSummary, String> {
    let mut summary = StreamSummary {
        samples: 0,
        bytes: 0,
        end_100ns: 0,
        first_timestamp: None,
        last_timestamp: None,
    };
    loop {
        let mut flags = 0_u32;
        let mut timestamp = 0_i64;
        let mut sample = None;
        unsafe {
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|error| format!("decode {name} sample: {error}"))?;
        }
        if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
            return Err(format!("Media Foundation reported a {name} stream error"));
        }
        if let Some(sample) = sample {
            if summary
                .last_timestamp
                .is_some_and(|previous| timestamp < previous)
            {
                return Err(format!("decoded {name} timestamps are not monotonic"));
            }
            summary.first_timestamp.get_or_insert(timestamp);
            summary.last_timestamp = Some(timestamp);
            summary.samples = summary
                .samples
                .checked_add(1)
                .ok_or_else(|| format!("{name} sample count overflow"))?;
            if summary.samples as usize > MAX_MEDIA_DECODED_SAMPLES {
                return Err(format!("{name} sample count exceeds worker limit"));
            }
            let length = unsafe { sample.GetTotalLength() }
                .map_err(|error| format!("measure decoded {name} sample: {error}"))?;
            if u64::from(length) > max_sample_bytes {
                return Err(format!("decoded {name} sample exceeds worker limit"));
            }
            summary.bytes = summary
                .bytes
                .checked_add(u64::from(length))
                .ok_or_else(|| format!("{name} byte count overflow"))?;
            if summary.bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
                return Err(format!("decoded {name} bytes exceed worker limit"));
            }
            let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0).max(0);
            let end = timestamp.saturating_add(duration).max(0) as u64;
            summary.end_100ns = summary.end_100ns.max(end);
            if summary.end_100ns > MAX_MEDIA_DURATION_100NS {
                return Err(format!("decoded {name} duration exceeds worker limit"));
            }
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    Ok(summary)
}
