use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn raster(
    clip: RectF,
    tile: RectF,
    gradient: &ResolvedGradient,
    radius: f32,
    repeat_x: bool,
    repeat_y: bool,
    width: u32,
    height: u32,
) -> Option<DecodedImage> {
    let mut pixmap = Pixmap::new(width, height)?;
    let scale = Transform::from_scale(width as f32 / clip.width, height as f32 / clip.height);
    let tile_width = (tile.width * width as f32 / clip.width).ceil().max(1.0) as u32;
    let tile_height = (tile.height * height as f32 / clip.height).ceil().max(1.0) as u32;
    if tile_width > MAX_DECODED_IMAGE_DIMENSION
        || tile_height > MAX_DECODED_IMAGE_DIMENSION
        || (u64::from(tile_width) * u64::from(tile_height) + u64::from(width) * u64::from(height))
            * 4
            > MAX_PAINT_BYTES as u64
    {
        return None;
    }
    let mut source = Pixmap::new(tile_width, tile_height)?;
    let shader = shader(gradient)?;
    let paint = Paint {
        shader,
        ..Paint::default()
    };
    source.fill_rect(
        skia::Rect::from_xywh(0.0, 0.0, tile.width, tile.height)?,
        &paint,
        Transform::from_scale(
            tile_width as f32 / tile.width,
            tile_height as f32 / tile.height,
        ),
        None,
    );
    let mut paint = Paint {
        shader: skia::Pattern::new(
            source.as_ref(),
            SpreadMode::Repeat,
            skia::FilterQuality::Bilinear,
            1.0,
            Transform::from_row(
                tile.width / tile_width as f32,
                0.0,
                0.0,
                tile.height / tile_height as f32,
                tile.x - clip.x,
                tile.y - clip.y,
            ),
        ),
        ..Paint::default()
    };
    paint.anti_alias = true;
    let mut mask = skia::Mask::new(width, height)?;
    mask.fill_path(
        &rounded_rect(clip.width, clip.height, radius)?,
        skia::FillRule::Winding,
        true,
        scale,
    );
    let x = if repeat_x {
        0.0
    } else {
        (tile.x - clip.x).max(0.0)
    };
    let y = if repeat_y {
        0.0
    } else {
        (tile.y - clip.y).max(0.0)
    };
    let right = if repeat_x {
        clip.width
    } else {
        (tile.right() - clip.x).min(clip.width)
    };
    let bottom = if repeat_y {
        clip.height
    } else {
        (tile.bottom() - clip.y).min(clip.height)
    };
    if let Some(rect) = skia::Rect::from_xywh(x, y, right - x, bottom - y) {
        pixmap.fill_rect(rect, &paint, scale, Some(&mask));
    }
    let mut bgra = pixmap.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(DecodedImage {
        width,
        height,
        bgra: bgra.into(),
    })
}

fn shader(gradient: &ResolvedGradient) -> Option<skia::Shader<'static>> {
    let stops = gradient
        .stops
        .iter()
        .map(|(offset, color)| {
            GradientStop::new(
                *offset,
                skia::Color::from_rgba8(color.red, color.green, color.blue, color.alpha),
            )
        })
        .collect();
    let spread = if gradient.repeating {
        SpreadMode::Repeat
    } else {
        SpreadMode::Pad
    };
    match gradient.geometry {
        Geometry::Solid(color) => Some(skia::Shader::SolidColor(skia::Color::from_rgba8(
            color.red,
            color.green,
            color.blue,
            color.alpha,
        ))),
        Geometry::Linear { start, end } => skia::LinearGradient::new(
            Point::from_xy(start[0], start[1]),
            Point::from_xy(end[0], end[1]),
            stops,
            spread,
            Transform::identity(),
        ),
        Geometry::Radial {
            center,
            radii,
            start_radius,
            end_radius,
        } => {
            if radii[0] <= 0.0 || radii[1] <= 0.0 {
                return None;
            }
            skia::RadialGradient::new(
                Point::from_xy(0.0, 0.0),
                start_radius,
                Point::from_xy(0.0, 0.0),
                end_radius,
                stops,
                spread,
                Transform::from_row(radii[0], 0.0, 0.0, radii[1], center[0], center[1]),
            )
        }
    }
}
