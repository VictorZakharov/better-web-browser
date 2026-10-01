//! Reuse Symphonia's vetted RFC 7845 identification reader.

use symphonia::core::io::BufReader;
use symphonia_common::xiph::audio::opus::OpusHead;

pub(crate) struct Header {
    pub(crate) channels: u16,
    pub(crate) pre_skip: u16,
    pub(crate) gain: i16,
    pub(crate) input_rate: u32,
}

pub(crate) fn read(bytes: &[u8]) -> Result<Header, String> {
    if bytes.len() > 256
        || bytes.get(18) != Some(&0)
        || !bytes.get(9).is_some_and(|count| matches!(count, 1 | 2))
    {
        return Err("WebM Opus requires a bounded family-0 mono/stereo OpusHead".into());
    }
    let head = OpusHead::read(&mut BufReader::new(bytes), 15)
        .map_err(|error| format!("invalid WebM Opus identification: {error}"))?;
    if head.version <= 1 && bytes.len() != 19 {
        return Err("WebM Opus identification has unexpected mapping bytes".into());
    }
    Ok(Header {
        channels: u16::from(bytes[9]),
        pre_skip: head.pre_skip,
        gain: head.gain,
        input_rate: head.original_sample_rate,
    })
}
