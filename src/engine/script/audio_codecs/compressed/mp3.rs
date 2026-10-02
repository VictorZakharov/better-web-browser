//! Minimal MPEG Layer III envelope and initial Xing/LAME priming admission.
//! Huffman decoding, synthesis, and reservoir state remain upstream-owned.
pub(super) struct Header {
    pub(super) mpeg1: bool,
    pub(super) mono: bool,
    pub(super) protected: bool,
}

pub(super) fn header(bytes: &[u8]) -> Result<Header, String> {
    let head = bytes.get(..4).ok_or("MP3 frame header is truncated")?;
    let version = (head[1] >> 3) & 3;
    let rate_index = (head[2] >> 2) & 3;
    let bitrate_index = head[2] >> 4;
    if head[0] != 255
        || head[1] & 0xe0 != 0xe0
        || version == 1
        || head[1] & 6 != 2
        || rate_index == 3
        || matches!(bitrate_index, 0 | 15)
        || head[3] & 3 == 2
    {
        return Err("MP3 requires a non-free-format MPEG-1/2/2.5 Layer III frame".into());
    }
    const MPEG1: [usize; 14] = [
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2: [usize; 14] = [8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let mpeg1 = version == 3;
    let bitrate = if mpeg1 { MPEG1 } else { MPEG2 }[usize::from(bitrate_index - 1)] * 1000;
    let rate = [44_100, 48_000, 32_000][usize::from(rate_index)]
        / match version {
            3 => 1,
            2 => 2,
            _ => 4,
        };
    let frame_bytes =
        (if mpeg1 { 144 } else { 72 }) * bitrate / rate + usize::from((head[2] >> 1) & 1);
    if bytes.len() != frame_bytes {
        return Err("MP3 chunk must contain exactly one complete frame".into());
    }
    Ok(Header {
        mpeg1,
        mono: head[3] >> 6 == 3,
        protected: head[1] & 1 == 0,
    })
}

pub(super) fn priming(bytes: &[u8], header: &Header) -> Result<usize, String> {
    let side = match (header.mpeg1, header.mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };
    let offset = 4 + usize::from(header.protected) * 2 + side;
    if !matches!(bytes.get(offset..offset + 4), Some(b"Xing" | b"Info")) {
        return Ok(0);
    }
    let flags = bytes
        .get(offset + 4..offset + 8)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or("MP3 Xing flags are truncated")?;
    if flags & !15 != 0 {
        return Err("MP3 Xing flags contain reserved bits".into());
    }
    let mut lame = offset + 8;
    for (bit, length) in [(1, 4), (2, 4), (4, 100), (8, 4)] {
        if flags & bit != 0 {
            lame += length;
        }
    }
    if lame > bytes.len() {
        return Err("MP3 Xing optional fields are truncated".into());
    }
    if bytes.get(lame..lame + 4) != Some(b"LAME") {
        return Ok(0);
    }
    let delay = bytes
        .get(lame + 21..lame + 24)
        .ok_or("MP3 LAME delay field is truncated")?;
    // The MP3 registration calls for the initial in-band encoder delay. Do not
    // add an invented fixed decoder delay or reuse later headers as reset cues.
    Ok((usize::from(delay[0]) << 4) | usize::from(delay[1] >> 4))
}
