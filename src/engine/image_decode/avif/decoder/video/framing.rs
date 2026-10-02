//! Low-overhead OBU envelope admission; rav1d owns AV1 syntax and reference decoding.
pub(super) fn validate(bytes: &[u8], key: bool) -> Result<(), String> {
    let mut offset = 0;
    let mut count = 0;
    let mut frame = false;
    while offset < bytes.len() {
        let header = bytes[offset];
        offset += 1;
        if header & 0x81 != 0 || header & 2 == 0 {
            return Err("AV1 packet requires bounded low-overhead OBUs with size fields".into());
        }
        let kind = (header >> 3) & 15;
        if !matches!(kind, 1..=8 | 15) {
            return Err("AV1 OBU type is reserved".into());
        }
        if header & 4 != 0 {
            let extension = *bytes.get(offset).ok_or("AV1 OBU extension is truncated")?;
            if extension & 7 != 0 {
                return Err("AV1 OBU extension has reserved bits".into());
            }
            offset += 1;
        }
        let mut size = 0_u64;
        let mut terminated = false;
        for shift in (0..56).step_by(7) {
            let value = *bytes.get(offset).ok_or("AV1 OBU size is truncated")?;
            offset += 1;
            size |= u64::from(value & 127) << shift;
            if value & 128 == 0 {
                terminated = true;
                break;
            }
        }
        if !terminated || size > u32::MAX as u64 {
            return Err("AV1 OBU size exceeds its registered bound".into());
        }
        offset = offset
            .checked_add(size as usize)
            .filter(|end| *end <= bytes.len())
            .ok_or("AV1 OBU payload is truncated")?;
        count += 1;
        if count > 256 {
            return Err("AV1 packet exceeds 256 OBUs".into());
        }
        frame |= matches!(kind, 3 | 6);
    }
    if key && !frame {
        return Err("AV1 key chunk lacks a frame header".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn low_overhead_framing_rejects_partial_and_annex_b_like_envelopes() {
        for bytes in [
            vec![0xff],
            vec![0x30, 0],
            vec![0x32],
            vec![0x32, 4, 0],
            vec![0x36],
            vec![0x36, 1, 0],
            vec![0x32, 0x80],
            vec![0x32, 0xff, 0xff, 0xff, 0xff, 0x7f],
        ] {
            assert!(validate(&bytes, true).is_err(), "{bytes:?}");
        }
    }
    #[test]
    fn headers_and_padding_alone_cannot_be_declared_a_key_chunk() {
        for bytes in [vec![0x12, 0], vec![0x7a, 0], vec![0x0a, 1, 0]] {
            assert!(validate(&bytes, true).is_err());
            assert!(validate(&bytes, false).is_ok());
        }
    }
    #[test]
    fn framing_checks_bounds_not_codec_payload_syntax() {
        // These deliberately opaque bytes are handed to rav1d, not interpreted
        // as successful decoded frames by this envelope validator.
        assert!(validate(&[0x32, 2, 9, 9], true).is_ok());
        assert!(validate(&[0x12, 0, 0x32, 1, 0], true).is_ok());
        assert!(validate(&[0x36, 0, 1, 0], true).is_ok());
    }
}
