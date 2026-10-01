//! The decoder owns chunk validation; this bounded walk only identifies APNG's
//! optional default image outside the animation (PNG Third Edition, §11.3.6).
pub(super) fn apng_has_poster(bytes: &[u8]) -> Result<bool, String> {
    let mut offset = 8usize;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or("truncated PNG chunk header")?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(length)
            .and_then(|value| value.checked_add(12))
            .ok_or("PNG chunk size overflow")?;
        if end > bytes.len() {
            return Err("truncated PNG chunk data".into());
        }
        match &header[4..8] {
            b"fcTL" => return Ok(false),
            b"IDAT" => return Ok(true),
            b"IEND" => return Err("PNG animation has no default image".into()),
            _ => {}
        }
        offset = end;
    }
    Err("PNG has no image data".into())
}

pub(super) fn orientation(value: image::metadata::Orientation) -> (u16, bool) {
    use image::metadata::Orientation::*;
    match value {
        NoTransforms => (0, false),
        FlipHorizontal => (0, true),
        Rotate180 => (180, false),
        FlipVertical => (180, true),
        Rotate90FlipH => (90, true),
        Rotate90 => (90, false),
        Rotate270FlipH => (270, true),
        Rotate270 => (270, false),
    }
}
