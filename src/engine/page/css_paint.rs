//! Generated CSS images stay inside the renderer and use bounded image-delta publication.
use super::*;
use crate::engine::css::images::gradient::geometry::{Geometry, ResolvedGradient};
use crate::engine::layout::{DisplayItem, LayoutOutput, RectF};
use crate::limits::MAX_DECODED_IMAGE_DIMENSION;
use resvg::tiny_skia::{
    self as skia, GradientStop, Paint, PathBuilder, Pixmap, Point, SpreadMode, Transform,
};
use sha2::{Digest, Sha256};

const PREFIX: &str = "breeze-css-paint:";
const MAX_PAINT_BYTES: usize = 64 * 1024 * 1024;
mod checkables;
mod gradients;
mod masks;
mod shadows;
mod shapes;
use gradients::raster;
#[cfg(test)]
mod tests;

impl Page {
    pub(crate) fn prepare_css_images(&mut self, layout: &mut LayoutOutput) {
        let scale = self.media_environment.resolution_dppx.clamp(0.1, 4.0);
        let key_for = |item: &DisplayItem| {
            masks::key(item, &self.images)
                .or_else(|| image_key(item))
                .map(|key| {
                    if matches!(
                        item,
                        DisplayItem::Image { .. } | DisplayItem::BeginMask { .. }
                    ) {
                        key
                    } else {
                        format!("{key}:dpr={scale}")
                    }
                })
        };
        let keys: Vec<_> = layout.items.iter().map(key_for).collect();
        let wanted: HashSet<_> = keys.iter().flatten().cloned().collect();
        // Release obsolete dimensions before admitting replacements; viewport changes
        // must not temporarily count both generations against the document budget.
        self.images
            .retain(|key, _| !key.starts_with(PREFIX) || wanted.contains(key));
        let mut active = HashSet::new();
        let mut charged = HashSet::new();
        let mut budget = 0usize;
        for (item, key) in layout.items.iter_mut().zip(keys) {
            let Some(key) = key else { continue };
            let (rect, valid) = paint_geometry(item);
            let width = (rect.width * scale).ceil().max(1.0) as u32;
            let height = (rect.height * scale).ceil().max(1.0) as u32;
            let bytes = u64::from(width) * u64::from(height) * 4;
            let accepted = valid
                && bytes <= MAX_PAINT_BYTES as u64
                && width <= MAX_DECODED_IMAGE_DIMENSION
                && height <= MAX_DECODED_IMAGE_DIMENSION
                && (charged.contains(&key)
                    || budget.saturating_add(bytes as usize) <= MAX_PAINT_BYTES);
            if accepted {
                if charged.insert(key.clone()) {
                    budget += bytes as usize;
                }
                if !self.images.contains_key(&key)
                    && let Some(image) = masks::raster(item, width, height, &self.images)
                        .or_else(|| paint_raster(item, width, height))
                    && let Err(error) = super::media::install_initial_decoded_image(
                        &mut self.images,
                        key.clone(),
                        image,
                    )
                {
                    let diagnostic = format!("CSS image omitted: {error}");
                    if !self.diagnostics.contains(&diagnostic) {
                        self.diagnostics.push(diagnostic);
                    }
                }
                if self.images.contains_key(&key) {
                    active.insert(key.clone());
                }
            }
            *item = if matches!(
                item,
                DisplayItem::BeginCssMask { .. } | DisplayItem::BeginMask { .. }
            ) {
                // Missing or over-budget masks remain hidden; never expose unmasked content.
                DisplayItem::BeginMask {
                    bounds: rect,
                    url: key,
                }
            } else if active.contains(&key) {
                DisplayItem::Image {
                    rect,
                    clip: Some(rect),
                    url: key,
                    alt: String::new(),
                    tint: None,
                }
            } else {
                DisplayItem::SolidRect {
                    rect,
                    color: crate::engine::css::Color::TRANSPARENT,
                    radius: 0.0,
                }
            };
        }
        self.images
            .retain(|key, _| !key.starts_with(PREFIX) || active.contains(key));
    }
}

fn paint_raster(item: &DisplayItem, width: u32, height: u32) -> Option<DecodedImage> {
    match item {
        DisplayItem::CssGradient {
            clip_rect,
            tile_rect,
            gradient,
            radius,
            repeat_x,
            repeat_y,
        } => raster(
            *clip_rect, *tile_rect, gradient, *radius, *repeat_x, *repeat_y, width, height,
        ),
        DisplayItem::CssBoxShadow { .. } => shadows::raster(item, width, height),
        DisplayItem::Control(spec) if spec.kind.is_checkable() => {
            checkables::raster(spec, width, height)
        }
        DisplayItem::SolidRect { .. } | DisplayItem::BorderRect { .. } => {
            shapes::raster(item, width, height)
        }
        _ => None,
    }
}

fn paint_geometry(item: &DisplayItem) -> (RectF, bool) {
    let (rect, values) = match item {
        DisplayItem::BeginCssMask { bounds, .. } | DisplayItem::BeginMask { bounds, .. } => {
            (*bounds, Vec::new())
        }
        DisplayItem::CssGradient {
            clip_rect,
            tile_rect,
            radius,
            ..
        } => (
            *clip_rect,
            vec![
                tile_rect.x,
                tile_rect.y,
                tile_rect.width,
                tile_rect.height,
                *radius,
            ],
        ),
        DisplayItem::CssBoxShadow {
            bounds,
            edge,
            offset,
            blur,
            spread,
            radius,
            ..
        } => (
            *bounds,
            vec![
                edge.x,
                edge.y,
                edge.width,
                edge.height,
                offset[0],
                offset[1],
                *blur,
                *spread,
                *radius,
            ],
        ),
        DisplayItem::Image { rect, url, .. } if url.starts_with(PREFIX) => (*rect, Vec::new()),
        DisplayItem::Control(spec) if spec.kind.is_checkable() => (spec.rect, Vec::new()),
        DisplayItem::SolidRect { rect, radius, .. }
        | DisplayItem::BorderRect { rect, radius, .. } => (*rect, vec![*radius]),
        _ => return (RectF::default(), false),
    };
    (
        rect,
        rect.width > 0.0
            && rect.height > 0.0
            && values
                .iter()
                .chain([rect.x, rect.y, rect.width, rect.height].iter())
                .all(|value| value.is_finite()),
    )
}

fn descriptor_key(
    clip: RectF,
    tile: RectF,
    gradient: &ResolvedGradient,
    radius: f32,
    repeat_x: bool,
    repeat_y: bool,
) -> String {
    format!(
        "{PREFIX}{:x}",
        Sha256::digest(
            format!(
                "{:?}:{:?}:{gradient:?}:{radius}:{repeat_x}:{repeat_y}",
                (clip.width, clip.height),
                (tile.x - clip.x, tile.y - clip.y, tile.width, tile.height)
            )
            .as_bytes()
        )
    )
}

fn image_key(item: &DisplayItem) -> Option<String> {
    if let Some(descriptor) = checkables::descriptor(item) {
        return Some(format!(
            "{PREFIX}widget:{:x}",
            Sha256::digest(descriptor.as_bytes())
        ));
    }
    if let DisplayItem::Image { url, .. } = item
        && url.starts_with(PREFIX)
    {
        return Some(url.clone());
    }
    if let Some(descriptor) = shapes::descriptor(item) {
        return Some(format!(
            "{PREFIX}shape:{:x}",
            Sha256::digest(descriptor.as_bytes())
        ));
    }
    if let DisplayItem::CssBoxShadow {
        bounds,
        edge,
        offset,
        blur,
        spread,
        radius,
        color,
        inset,
    } = item
    {
        return Some(format!(
            "{PREFIX}shadow:{:x}",
            Sha256::digest(
                format!(
                    "{:?}:{:?}:{offset:?}:{blur}:{spread}:{radius}:{color:?}:{inset}",
                    (bounds.width, bounds.height),
                    (
                        edge.x - bounds.x,
                        edge.y - bounds.y,
                        edge.width,
                        edge.height
                    )
                )
                .as_bytes()
            )
        ));
    }
    if let DisplayItem::CssGradient {
        clip_rect,
        tile_rect,
        gradient,
        radius,
        repeat_x,
        repeat_y,
    } = item
    {
        Some(descriptor_key(
            *clip_rect, *tile_rect, gradient, *radius, *repeat_x, *repeat_y,
        ))
    } else {
        None
    }
}

fn rounded_rect(width: f32, height: f32, radius: f32) -> Option<skia::Path> {
    let r = radius.max(0.0).min(width / 2.0).min(height / 2.0);
    let k = r * 0.552_284_8;
    let mut path = PathBuilder::new();
    path.move_to(r, 0.0);
    path.line_to(width - r, 0.0);
    path.cubic_to(width - r + k, 0.0, width, r - k, width, r);
    path.line_to(width, height - r);
    path.cubic_to(
        width,
        height - r + k,
        width - r + k,
        height,
        width - r,
        height,
    );
    path.line_to(r, height);
    path.cubic_to(r - k, height, 0.0, height - r + k, 0.0, height - r);
    path.line_to(0.0, r);
    path.cubic_to(0.0, r - k, r - k, 0.0, r, 0.0);
    path.close();
    path.finish()
}
