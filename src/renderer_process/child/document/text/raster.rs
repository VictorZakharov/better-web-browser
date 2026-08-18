use crate::engine::DecodedImage;
use crate::limits::{
    MAX_GLYPH_RASTER_BYTES, MAX_GLYPH_RASTER_DIMENSION, MAX_GLYPH_RASTER_PIXELS, MAX_GLYPH_RASTERS,
    MAX_PRESENTED_GLYPH_BYTES,
};
use crate::renderer_protocol::PresentedGlyphRaster;
use cosmic_text::{CacheKey, FontSystem, SwashCache, SwashContent, SwashImage};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct GlyphAsset {
    pub(super) id: u32,
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color: bool,
}

pub(super) struct GlyphRasterCache {
    swash: SwashCache,
    assets: HashMap<CacheKey, GlyphAsset>,
    pending: Vec<PresentedGlyphRaster>,
    bytes: usize,
}

impl Default for GlyphRasterCache {
    fn default() -> Self {
        Self {
            swash: SwashCache::new(),
            assets: HashMap::new(),
            pending: Vec::new(),
            bytes: 0,
        }
    }
}

impl GlyphRasterCache {
    pub(super) fn get_or_insert(
        &mut self,
        fonts: &mut FontSystem,
        key: CacheKey,
    ) -> Option<GlyphAsset> {
        if let Some(asset) = self.assets.get(&key) {
            return Some(*asset);
        }
        if self.assets.len() >= MAX_GLYPH_RASTERS {
            return None;
        }
        let image = self.swash.get_image_uncached(fonts, key)?;
        let decoded = decode_swash_image(&image)?;
        let next_bytes = self.bytes.checked_add(decoded.bgra.len())?;
        if next_bytes > MAX_PRESENTED_GLYPH_BYTES {
            return None;
        }
        let id = u32::try_from(self.assets.len()).ok()?.checked_add(1)?;
        let color = image.content == SwashContent::Color;
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
        Some(asset)
    }

    pub(super) fn take_pending(&mut self) -> Vec<PresentedGlyphRaster> {
        std::mem::take(&mut self.pending)
    }

    pub(super) fn clear(&mut self) {
        self.swash = SwashCache::new();
        self.assets.clear();
        self.pending.clear();
        self.bytes = 0;
    }
}

fn decode_swash_image(image: &SwashImage) -> Option<DecodedImage> {
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
        SwashContent::Mask => decode_mask(&image.data, output_bytes)?,
        SwashContent::Color => decode_color(&image.data, output_bytes)?,
        SwashContent::SubpixelMask => decode_subpixel_mask(&image.data, output_bytes)?,
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
