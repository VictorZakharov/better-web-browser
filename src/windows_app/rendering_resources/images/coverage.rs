//! A conservative, bounded cover of nonzero BGRA tiles for unscaled source-over draws.
//! Skip only all-zero bytes: malformed non-premultiplied RGB with alpha zero must
//! retain AlphaBlend's behavior too. Rectangles never overlap or change sampling.
use super::*;
use std::sync::Arc;
const TILE: usize = 64;
const LIMIT: usize = 16;

pub(super) fn derive(image: &DecodedImage) -> Option<Arc<[Rect]>> {
    if !valid(image) {
        return None;
    }
    derive_bytes(image.width, image.height, &image.bgra)
}

pub(super) fn derive_bytes(width: u32, height: u32, bytes: &[u8]) -> Option<Arc<[Rect]>> {
    if width < 128
        || height < 128
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?
            != bytes.len()
    {
        return None;
    }
    let width = width as usize;
    let height = height as usize;
    let mut rectangles: Vec<Rect> = Vec::new();
    let mut previous = Vec::new();
    for y in (0..height).step_by(TILE) {
        let bottom = (y + TILE).min(height);
        let mut active = Vec::new();
        let mut start = None;
        for x in (0..width).step_by(TILE) {
            let right = (x + TILE).min(width);
            let nonzero = (y..bottom).any(|row| {
                bytes[(row * width + x) * 4..(row * width + right) * 4]
                    .iter()
                    .any(|byte| *byte != 0)
            });
            if nonzero && start.is_none() {
                start = Some(x);
            }
            if !nonzero && let Some(left) = start.take() {
                active.push((left, x));
            }
        }
        if let Some(left) = start {
            active.push((left, width));
        }
        let mut current = Vec::new();
        for (left, right) in active {
            let matching = previous.iter().copied().find(|index: &usize| {
                let rect = rectangles[*index];
                rect.left == left as i32 && rect.right == right as i32 && rect.bottom == y as i32
            });
            let index = if let Some(index) = matching {
                rectangles[index].bottom = bottom as i32;
                index
            } else {
                if rectangles.len() == LIMIT {
                    return None;
                }
                rectangles.push(Rect {
                    left: left as i32,
                    top: y as i32,
                    right: right as i32,
                    bottom: bottom as i32,
                });
                rectangles.len() - 1
            };
            current.push(index);
        }
        previous = current;
    }
    let covered: u64 = rectangles
        .iter()
        .map(|rect| rect.width() as u64 * rect.height() as u64)
        .sum();
    (covered * 2 < width as u64 * height as u64).then(|| rectangles.into())
}

#[cfg(test)]
mod tests;
