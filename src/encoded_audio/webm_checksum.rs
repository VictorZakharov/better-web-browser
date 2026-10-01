//! RFC 8794 §11.3.1: one first-child IEEE CRC-32, stored little-endian and
//! covering its parent's encoded data except the CRC element itself.

use super::super::Budget;

pub(super) fn validate(
    first_child: bool,
    payload: &[u8],
    remainder: &[u8],
    budget: &mut Budget<'_>,
) -> Result<(), String> {
    if !first_child {
        return Err("WebM CRC-32 must be a unique first child of a master".into());
    }
    let stored = u32::from_le_bytes(
        payload
            .try_into()
            .map_err(|_| "WebM CRC-32 must contain four bytes")?,
    );
    let mut crc = crc32fast::Hasher::new();
    // A complete parent is already bounded by source length and depth. Chunk
    // checksum work as well, so cancellation is not postponed across a large
    // metadata subtree. No second copy of the parent is allocated.
    for chunk in remainder.chunks(64 * 1024) {
        budget.step()?;
        crc.update(chunk);
    }
    if crc.finalize() != stored {
        return Err("WebM CRC-32 does not match its parent data".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_uses_ieee_polynomial_and_little_endian_storage() {
        let mut budget = Budget::new(None);
        // Published IEEE check value for the standard ASCII test vector.
        validate(true, &[0x26, 0x39, 0xf4, 0xcb], b"123456789", &mut budget).unwrap();
        assert!(validate(true, &[0xcb, 0xf4, 0x39, 0x26], b"123456789", &mut budget).is_err());
        validate(true, &[0; 4], b"", &mut budget).unwrap();
    }

    #[test]
    fn malformed_shape_and_cancellation_are_not_checksum_success() {
        let mut budget = Budget::new(None);
        assert!(validate(false, &[0; 4], b"", &mut budget).is_err());
        for width in [0, 1, 2, 3, 5, 8] {
            assert!(validate(true, &vec![0; width], b"", &mut budget).is_err());
        }
        let flag = std::sync::atomic::AtomicBool::new(true);
        let mut budget = Budget::new(Some(&flag));
        assert!(
            validate(true, &[0; 4], &vec![0; 128 * 1024], &mut budget)
                .unwrap_err()
                .contains("cancel")
        );
    }
}
