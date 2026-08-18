//! Direct Swash rasterization for renderer-owned shaped glyphs.

use super::catalog::{FontInstanceKey, SelectedFont};
use crate::engine::DecodedImage;
use crate::limits::{
    MAX_GLYPH_RASTER_BYTES, MAX_GLYPH_RASTER_DIMENSION, MAX_GLYPH_RASTER_PIXELS, MAX_GLYPH_RASTERS,
    MAX_PRESENTED_GLYPH_BYTES,
};
use crate::renderer_protocol::PresentedGlyphRaster;
use std::collections::HashMap;
use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
use swash::zeno::{Angle, Format, Transform, Vector};

#[derive(Clone, Copy)]
pub(super) struct GlyphAsset {
    pub(super) id: u32,
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color: bool,
}

pub(super) struct RasterizedGlyph {
    pub(super) asset: GlyphAsset,
    pub(super) origin_x: i32,
    pub(super) origin_y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RasterKey {
    font: FontInstanceKey,
    glyph_id: u16,
    size: u32,
    x_bin: u8,
    y_bin: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FontDataKey {
    blob_id: u64,
    index: u32,
}

pub(super) struct GlyphRasterCache {
    context: ScaleContext,
    font_keys: HashMap<FontDataKey, swash::CacheKey>,
    assets: HashMap<RasterKey, GlyphAsset>,
    pending: Vec<PresentedGlyphRaster>,
    bytes: usize,
}

impl Default for GlyphRasterCache {
    fn default() -> Self {
        Self {
            context: ScaleContext::new(),
            font_keys: HashMap::new(),
            assets: HashMap::new(),
            pending: Vec::new(),
            bytes: 0,
        }
    }
}

impl GlyphRasterCache {
    pub(super) fn get_or_insert(
        &mut self,
        font: &SelectedFont,
        glyph_id: u16,
        size: f32,
        x: f32,
        baseline: f32,
        dpi: u32,
    ) -> Option<RasterizedGlyph> {
        let scale = dpi.max(1) as f32 / 96.0;
        let (origin_x, x_bin, x_offset) = quantize_subpixel(x * scale);
        let (origin_y, y_bin, y_offset) = quantize_subpixel(baseline * scale);
        let physical_size = (size * scale).clamp(1.0, 1536.0);
        let key = RasterKey {
            font: font.instance,
            glyph_id,
            size: physical_size.to_bits(),
            x_bin,
            y_bin,
        };
        if let Some(asset) = self.assets.get(&key) {
            return Some(RasterizedGlyph {
                asset: *asset,
                origin_x,
                origin_y,
            });
        }
        if self.assets.len() >= MAX_GLYPH_RASTERS {
            return None;
        }
        let image = self.render(font, glyph_id, physical_size, x_offset, y_offset)?;
        let decoded = decode_swash_image(&image)?;
        let next_bytes = self.bytes.checked_add(decoded.bgra.len())?;
        if next_bytes > MAX_PRESENTED_GLYPH_BYTES {
            return None;
        }
        let id = u32::try_from(self.assets.len()).ok()?.checked_add(1)?;
        let color = image.content == Content::Color;
        let asset = GlyphAsset {
            id,
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            color,
        };
        self.bytes = next_bytes;
        self.assets.insert(key, asset);
        self.pending.push(PresentedGlyphRaster {
            id,
            image: decoded,
            color,
        });
        Some(RasterizedGlyph {
            asset,
            origin_x,
            origin_y,
        })
    }

    fn render(
        &mut self,
        selected: &SelectedFont,
        glyph_id: u16,
        size: f32,
        x_offset: f32,
        y_offset: f32,
    ) -> Option<swash::scale::image::Image> {
        let mut font =
            swash::FontRef::from_index(selected.font.blob.as_ref(), selected.font.index as usize)?;
        let data_key = FontDataKey {
            blob_id: selected.font.blob.id(),
            index: selected.font.index,
        };
        font.key = *self.font_keys.entry(data_key).or_insert(font.key);
        let settings = selected
            .font
            .synthesis
            .variation_settings()
            .iter()
            .map(|(tag, value)| (swash::Tag::from_be_bytes(tag.to_be_bytes()), *value));
        let coords = font
            .variations()
            .normalized_coords(settings)
            .collect::<Vec<_>>();
        let mut builder = self.context.builder(font).size(size).hint(true);
        if !coords.is_empty() {
            builder = builder.normalized_coords(&coords);
        }
        let mut scaler = builder.build();
        let transform =
            selected.font.synthesis.skew().map(|degrees| {
                Transform::skew(Angle::from_degrees(degrees), Angle::from_degrees(0.0))
            });
        Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .offset(Vector::new(x_offset, y_offset))
        .transform(transform)
        .embolden(if selected.font.synthesis.embolden() {
            size * 0.02
        } else {
            0.0
        })
        .render(&mut scaler, glyph_id)
    }

    pub(super) fn take_pending(&mut self) -> Vec<PresentedGlyphRaster> {
        std::mem::take(&mut self.pending)
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

fn quantize_subpixel(value: f32) -> (i32, u8, f32) {
    let mut origin = value.floor() as i32;
    let mut bin = ((value - origin as f32) * 4.0).round() as i32;
    if bin >= 4 {
        origin = origin.saturating_add(1);
        bin = 0;
    }
    let bin = bin.clamp(0, 3) as u8;
    (origin, bin, bin as f32 * 0.25)
}

fn decode_swash_image(image: &swash::scale::image::Image) -> Option<DecodedImage> {
    let width = image.placement.width;
    let height = image.placement.height;
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    let output_bytes = usize::try_from(pixels.checked_mul(4)?).ok()?;
    if width == 0
        || height == 0
        || width > MAX_GLYPH_RASTER_DIMENSION
        || height > MAX_GLYPH_RASTER_DIMENSION
        || pixels > MAX_GLYPH_RASTER_PIXELS
        || output_bytes > MAX_GLYPH_RASTER_BYTES
    {
        return None;
    }
    let bgra = match image.content {
        Content::Mask => decode_mask(&image.data, output_bytes)?,
        Content::Color => decode_color(&image.data, output_bytes)?,
        Content::SubpixelMask => decode_subpixel_mask(&image.data, output_bytes)?,
    };
    Some(DecodedImage {
        width,
        height,
        bgra,
    })
}

fn decode_mask(source: &[u8], output_bytes: usize) -> Option<Vec<u8>> {
    if source.len().checked_mul(4)? != output_bytes {
        return None;
    }
    let mut output = Vec::with_capacity(output_bytes);
    for alpha in source {
        output.extend_from_slice(&[*alpha, *alpha, *alpha, *alpha]);
    }
    Some(output)
}

fn decode_color(source: &[u8], output_bytes: usize) -> Option<Vec<u8>> {
    if source.len() != output_bytes {
        return None;
    }
    let mut output = Vec::with_capacity(output_bytes);
    for rgba in source.chunks_exact(4) {
        let alpha = u16::from(rgba[3]);
        output.push((u16::from(rgba[2]) * alpha / 255) as u8);
        output.push((u16::from(rgba[1]) * alpha / 255) as u8);
        output.push((u16::from(rgba[0]) * alpha / 255) as u8);
        output.push(rgba[3]);
    }
    Some(output)
}

fn decode_subpixel_mask(source: &[u8], output_bytes: usize) -> Option<Vec<u8>> {
    if source.len().checked_mul(4)? != output_bytes.checked_mul(3)? {
        return None;
    }
    let mut output = Vec::with_capacity(output_bytes);
    for rgb in source.chunks_exact(3) {
        let alpha = *rgb.iter().max()?;
        output.extend_from_slice(&[alpha, alpha, alpha, alpha]);
    }
    Some(output)
}
