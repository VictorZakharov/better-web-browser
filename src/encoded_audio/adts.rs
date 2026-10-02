//! Admission for complete ADTS AAC-LC files; Symphonia owns packet extraction.

const SAMPLE_RATES: [u32; 13] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
    7_350,
];

pub(crate) struct Frame<'a> {
    pub(crate) rate: u32,
    pub(crate) channels: u32,
    pub(crate) payload: &'a [u8],
    pub(crate) bytes: usize,
    pub(crate) protected: bool,
}

/// Complete single-raw-block AAC-LC framing shared by file and packet consumers.
/// A CRC is framing, not verification: callers decide their integrity policy.
pub(crate) fn frame(bytes: &[u8]) -> Result<Frame<'_>, String> {
    let head = bytes.get(..7).ok_or("ADTS frame header is truncated")?;
    if head[0] != 0xff || head[1] & 0xf6 != 0xf0 {
        return Err("ADTS frame synchronization or layer is invalid".into());
    }
    let rate = SAMPLE_RATES
        .get(usize::from((head[2] >> 2) & 15))
        .copied()
        .ok_or("ADTS sample-rate index is reserved")?;
    let channels = u32::from(((head[2] & 1) << 2) | (head[3] >> 6));
    if head[2] >> 6 != 1 || rate < 8_000 || channels == 0 || head[6] & 3 != 0 {
        return Err("ADTS source requires supported AAC-LC with one raw block per frame".into());
    }
    let protected = head[1] & 1 == 0;
    let header_bytes = if protected { 9 } else { 7 };
    let frame_bytes =
        (usize::from(head[3] & 3) << 11) | (usize::from(head[4]) << 3) | usize::from(head[5] >> 5);
    if frame_bytes <= header_bytes || frame_bytes > bytes.len() {
        return Err("ADTS frame payload is empty or truncated".into());
    }
    Ok(Frame {
        rate,
        channels,
        payload: &bytes[header_bytes..frame_bytes],
        bytes: frame_bytes,
        protected,
    })
}

pub(super) fn validate(bytes: &[u8], budget: &mut super::Budget<'_>) -> Result<(), String> {
    let mut remaining = bytes;
    let mut format = None;
    let mut frames = 0_usize;
    while !remaining.is_empty() {
        budget.step()?;
        let packet = frame(remaining)?;
        let current = (packet.rate, packet.channels);
        if format.is_some_and(|previous| previous != current) {
            return Err("ADTS audio format changes within the source".into());
        }
        format = Some(current);
        // Symphonia reads/skips the optional ADTS CRC but does not verify it.
        // This policy proves complete framing, not that transport checksum.
        frames += 1;
        if frames > crate::limits::MAX_MEDIA_DECODED_SAMPLES {
            return Err("ADTS frame count exceeds the decode limit".into());
        }
        remaining = &remaining[packet.bytes..];
    }
    if frames == 0 {
        return Err("ADTS source has no frames".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    fn validate(bytes: &[u8]) -> Result<(), String> {
        crate::encoded_audio::validate(bytes, crate::encoded_audio::Kind::AacAdts)
    }

    fn frame() -> Vec<u8> {
        vec![0xff, 0xf1, 0x50, 0x40, 1, 0x1f, 0xfc, 0]
    }

    #[test]
    fn rejects_every_partial_frame_header_or_payload() {
        let good = frame();
        assert!(validate(&good).is_ok());
        for end in 0..good.len() {
            assert!(validate(&good[..end]).is_err(), "{end}");
        }
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(validate(&trailing).is_err());
    }

    #[test]
    fn rejects_reserved_rates_profiles_and_unimplemented_block_shapes() {
        for byte in [0x10, 0x90, 0xd0, 0x74, 0x70] {
            let mut source = frame();
            source[2] = byte;
            assert!(validate(&source).is_err(), "{byte:x}");
        }
        let mut multi_block = frame();
        multi_block[6] |= 1;
        assert!(validate(&multi_block).is_err());
        let mut unknown_channels = frame();
        unknown_channels[3] = 0;
        assert!(validate(&unknown_channels).is_err());
    }

    #[test]
    fn rejects_a_format_change_in_an_otherwise_complete_second_frame() {
        let mut source = frame();
        let mut second = frame();
        second[2] = 0x4c; // 48 kHz after 44.1 kHz.
        source.extend(second);
        assert!(validate(&source).is_err());
    }

    #[test]
    fn declared_length_cannot_wrap_or_exceed_available_bytes() {
        let mut source = frame();
        source[3] |= 3;
        source[4] = 255;
        source[5] |= 0xe0;
        assert!(validate(&source).is_err());
        let mut crc_truncated = frame();
        crc_truncated[1] &= !1;
        assert!(validate(&crc_truncated).is_err());
    }
}
