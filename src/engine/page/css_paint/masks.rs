//! Alpha mask composition. RGB values never affect mask work or alpha coverage.
use super::*;

pub(super) fn key(item: &DisplayItem, images: &HashMap<String, DecodedImage>) -> Option<String> {
    if let DisplayItem::BeginMask { url, .. } = item {
        return Some(url.clone());
    }
    let DisplayItem::BeginCssMask { bounds, layers } = item else {
        return None;
    };
    let mut descriptor = layers.clone();
    crate::engine::layout::translate_display_items(&mut descriptor, -bounds.x, -bounds.y);
    let mut hash = Sha256::new();
    hash.update(format!(
        "{:?}:{descriptor:?}",
        (bounds.width, bounds.height)
    ));
    for layer in layers {
        if let DisplayItem::BackgroundImage { url, .. } = layer {
            // Immutable resource identity changes when same-URL pixels are replaced.
            if let Some(image) = images.get(url) {
                hash.update(format!(
                    "{}:{}:{:p}",
                    image.width,
                    image.height,
                    image.bgra.as_ptr()
                ));
            }
        }
    }
    Some(format!("{PREFIX}mask:{:x}", hash.finalize()))
}

pub(super) fn raster(
    item: &DisplayItem,
    width: u32,
    height: u32,
    images: &HashMap<String, DecodedImage>,
) -> Option<DecodedImage> {
    let DisplayItem::BeginCssMask { bounds, layers } = item else {
        return None;
    };
    if layers.len() > 32 {
        return None;
    }
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for layer in layers {
        if let DisplayItem::BackgroundImage {
            clip_rect,
            tile_rect,
            url,
            repeat_x,
            repeat_y,
        } = layer
        {
            let Some(source) = images.get(url) else {
                continue;
            };
            // SVG <mask> references are not ordinary SVG images. Leave unsupported
            // references transparent rather than misinterpret them as root SVG pixels.
            let base = url
                .split('#')
                .next()
                .unwrap_or(url)
                .split('?')
                .next()
                .unwrap_or(url);
            if url.contains('#')
                && (base.ends_with(".svg") || base.starts_with("data:image/svg+xml"))
            {
                continue;
            }
            if !valid_tile(*tile_rect)
                || source.bgra.len() != source.width as usize * source.height as usize * 4
            {
                continue;
            }
            for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                let x = bounds.x
                    + (i % width as usize) as f32 * bounds.width / width as f32
                    + bounds.width / width as f32 * 0.5;
                let y = bounds.y
                    + (i / width as usize) as f32 * bounds.height / height as f32
                    + bounds.height / height as f32 * 0.5;
                let alpha = sample(source, *clip_rect, *tile_rect, [x, y], *repeat_x, *repeat_y);
                pixel[3] = add(pixel[3], alpha);
            }
        } else if let Some(source) = paint_raster(layer, width, height) {
            for (pixel, mask) in pixels.chunks_exact_mut(4).zip(source.bgra.chunks_exact(4)) {
                pixel[3] = add(pixel[3], mask[3]);
            }
        }
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[0] = pixel[3];
        pixel[1] = pixel[3];
        pixel[2] = pixel[3];
    }
    Some(DecodedImage {
        width,
        height,
        bgra: pixels.into(),
    })
}

fn add(destination: u8, source: u8) -> u8 {
    (u16::from(source) + u16::from(destination)
        - (u16::from(source) * u16::from(destination) + 127) / 255) as u8
}

fn valid_tile(tile: RectF) -> bool {
    [tile.x, tile.y, tile.width, tile.height]
        .iter()
        .all(|v| v.is_finite())
        && tile.width > 0.0
        && tile.height > 0.0
}

fn sample(
    source: &DecodedImage,
    clip: RectF,
    tile: RectF,
    point: [f32; 2],
    repeat_x: bool,
    repeat_y: bool,
) -> u8 {
    let [x, y] = point;
    if x < clip.x
        || x >= clip.right()
        || y < clip.y
        || y >= clip.bottom()
        || (!repeat_x && (x < tile.x || x >= tile.right()))
        || (!repeat_y && (y < tile.y || y >= tile.bottom()))
        || source.width == 0
        || source.height == 0
    {
        return 0;
    }
    let x = (x - tile.x).rem_euclid(tile.width) * source.width as f32 / tile.width - 0.5;
    let y = (y - tile.y).rem_euclid(tile.height) * source.height as f32 / tile.height - 0.5;
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let at = |x: i64, y: i64| {
        let axis = |v: i64, max: u32, repeat| {
            if repeat {
                v.rem_euclid(i64::from(max)) as usize
            } else {
                v.clamp(0, i64::from(max) - 1) as usize
            }
        };
        source.bgra[(axis(y, source.height, repeat_y) * source.width as usize
            + axis(x, source.width, repeat_x))
            * 4
            + 3] as f32
    };
    let fx = x - x.floor();
    let fy = y - y.floor();
    ((at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx) * (1.0 - fy)
        + (at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx) * fy)
        .round() as u8
}

#[cfg(test)]
mod tests;
