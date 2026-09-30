//! Bound length-driven upstream FLAC metadata allocations before parsing them.

use base64::Engine as _;

pub(super) fn validate(kind: u8, bytes: &[u8]) -> Result<(), String> {
    match kind {
        2 if bytes.len() < 4 => return Err("FLAC application metadata lacks its identifier".into()),
        3 if !bytes.len().is_multiple_of(18) => {
            return Err("FLAC seek table length is invalid".into());
        }
        4 => {
            let end = crate::ogg_vorbis_headers::preflight_comment_fields(bytes, |comment| {
                let Some(equal) = comment.iter().position(|byte| *byte == b'=') else {
                    return Ok(());
                };
                if comment[..equal].eq_ignore_ascii_case(b"METADATA_BLOCK_PICTURE") {
                    let encoded = &comment[equal + 1..];
                    let picture = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| "FLAC comment picture has invalid Base64")?;
                    picture_lengths(&picture)?;
                }
                Ok(())
            })?;
            if end != bytes.len() {
                return Err("FLAC comment block has trailing bytes".into());
            }
        }
        6 => picture_lengths(bytes)?,
        _ => {}
    }
    Ok(())
}

fn picture_lengths(bytes: &[u8]) -> Result<(), String> {
    let mut offset = 4_usize; // picture type
    take_field(bytes, &mut offset)?; // media type
    take_field(bytes, &mut offset)?; // UTF-8 description
    offset = offset
        .checked_add(16)
        .filter(|end| *end <= bytes.len())
        .ok_or("FLAC picture dimensions are truncated")?;
    take_field(bytes, &mut offset)?; // image data
    if offset != bytes.len() {
        return Err("FLAC picture block has trailing bytes".into());
    }
    Ok(())
}

fn take_field(bytes: &[u8], offset: &mut usize) -> Result<(), String> {
    let encoded: [u8; 4] = bytes
        .get(*offset..offset.saturating_add(4))
        .and_then(|value| value.try_into().ok())
        .ok_or("FLAC picture length is truncated")?;
    *offset = offset
        .checked_add(4)
        .and_then(|start| start.checked_add(u32::from_be_bytes(encoded) as usize))
        .filter(|end| *end <= bytes.len())
        .ok_or("FLAC picture field exceeds metadata block")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_lengths_are_checked_before_upstream_allocations() {
        let mut picture = vec![0; 32];
        assert!(picture_lengths(&picture).is_ok());
        for end in 0..picture.len() {
            assert!(picture_lengths(&picture[..end]).is_err());
        }
        for position in [4, 8, 28] {
            picture[position..position + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(picture_lengths(&picture).is_err());
            picture[position..position + 4].fill(0);
        }
    }

    #[test]
    fn comments_reject_impossible_counts_lengths_and_embedded_pictures() {
        assert!(validate(4, &u32::MAX.to_le_bytes()).is_err());
        let mut comment = vec![0; 4];
        comment.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate(4, &comment).is_err());
        let mut picture = vec![0; 8];
        picture[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        let field = format!(
            "METADATA_BLOCK_PICTURE={}",
            base64::engine::general_purpose::STANDARD.encode(picture)
        );
        let mut comment = vec![0; 4];
        comment.extend_from_slice(&1_u32.to_le_bytes());
        comment.extend_from_slice(&(field.len() as u32).to_le_bytes());
        comment.extend_from_slice(field.as_bytes());
        assert!(validate(4, &comment).is_err());
        assert!(validate(2, &[1, 2, 3]).is_err());
        assert!(validate(3, &[0; 17]).is_err());
    }
}
