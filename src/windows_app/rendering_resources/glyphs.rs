//! Cached text-run surfaces assembled from renderer-owned glyph rasters.

use super::super::paint_primitives::bitmap_info;
use super::super::*;
use better_web_browser::engine::PositionedGlyph;
use better_web_browser::renderer_protocol::PresentedGlyphRaster;
use std::collections::VecDeque;

const MAX_RUN_DIMENSION: u32 = 8_192;
const MAX_RUN_PIXELS: u64 = 4 * 1024 * 1024;
const MAX_CACHED_RUN_BYTES: usize = 24 * 1024 * 1024;
const MAX_CACHED_RUNS: usize = 4_096;

#[derive(Clone, Copy)]
pub(in crate::windows_app) struct GlyphRunBitmap {
    pub(in crate::windows_app) bitmap: Hbitmap,
    pub(in crate::windows_app) offset_x: f32,
    pub(in crate::windows_app) offset_y: f32,
    pub(in crate::windows_app) width: f32,
    pub(in crate::windows_app) height: f32,
    pub(in crate::windows_app) source_width: u32,
    pub(in crate::windows_app) source_height: u32,
}

#[derive(Default)]
pub(in crate::windows_app) struct GlyphBitmaps {
    glyphs: HashMap<(u32, Option<[u8; 4]>), Hbitmap>,
    runs: HashMap<(u64, [u8; 4], u32), CachedGlyphRun>,
    run_order: VecDeque<(u64, [u8; 4], u32)>,
    run_bytes: usize,
}

struct CachedGlyphRun {
    bitmap: GlyphRunBitmap,
    bytes: usize,
    glyphs: Vec<PositionedGlyph>,
}

impl GlyphBitmaps {
    pub(in crate::windows_app) unsafe fn get_or_create_run(
        &mut self,
        run_id: u64,
        glyphs: &[PositionedGlyph],
        resources: &HashMap<u32, PresentedGlyphRaster>,
        tint: [u8; 4],
        scale: f32,
        dc: Hdc,
    ) -> Option<GlyphRunBitmap> {
        if run_id == 0 || glyphs.is_empty() || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let key = (run_id, tint, scale.to_bits());
        if let Some(run) = self.runs.get(&key)
            && run.glyphs == glyphs
        {
            return Some(run.bitmap);
        }
        // Run IDs are renderer-provided optimization hints, not browser authority. If a
        // compromised renderer reuses one for different placements, discard the stale surface.
        if let Some(stale) = self.runs.remove(&key) {
            self.run_order.retain(|candidate| *candidate != key);
            self.run_bytes = self.run_bytes.saturating_sub(stale.bytes);
            DeleteObject(stale.bitmap.bitmap);
        }

        let bounds = pixel_bounds(glyphs, resources, scale)?;
        let width = u32::try_from(bounds.right.checked_sub(bounds.left)?).ok()?;
        let height = u32::try_from(bounds.bottom.checked_sub(bounds.top)?).ok()?;
        let pixels = u64::from(width).checked_mul(u64::from(height))?;
        if width == 0
            || height == 0
            || width > MAX_RUN_DIMENSION
            || height > MAX_RUN_DIMENSION
            || pixels > MAX_RUN_PIXELS
        {
            return None;
        }
        let bitmap_bytes = usize::try_from(pixels.checked_mul(4)?).ok()?;
        let byte_count = bitmap_bytes.checked_add(
            glyphs
                .len()
                .checked_mul(std::mem::size_of::<PositionedGlyph>())?,
        )?;
        if byte_count > MAX_CACHED_RUN_BYTES {
            return None;
        }
        while self.run_bytes.saturating_add(byte_count) > MAX_CACHED_RUN_BYTES
            || self.runs.len() >= MAX_CACHED_RUNS
        {
            let oldest = self.run_order.pop_front()?;
            if let Some(expired) = self.runs.remove(&oldest) {
                self.run_bytes = self.run_bytes.saturating_sub(expired.bytes);
                DeleteObject(expired.bitmap.bitmap);
            }
        }
        let mut bgra = vec![0_u8; bitmap_bytes];
        let tint_table = mask_tint_table(tint);
        for glyph in glyphs {
            let resource = resources.get(&glyph.raster_id)?;
            if resource.color != glyph.color {
                return None;
            }
            let left = scaled_coordinate(glyph.x, scale)?.checked_sub(bounds.left)?;
            let top = scaled_coordinate(glyph.y, scale)?.checked_sub(bounds.top)?;
            composite_glyph(&mut bgra, width, height, left, top, resource, &tint_table)?;
        }

        let image = DecodedImage {
            width,
            height,
            bgra,
        };
        let info = bitmap_info(&image);
        let mut destination = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut destination, null_mut(), 0);
        if bitmap.is_null() || destination.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            return None;
        }
        std::ptr::copy_nonoverlapping(image.bgra.as_ptr(), destination.cast(), image.bgra.len());
        let run = GlyphRunBitmap {
            bitmap,
            offset_x: bounds.left as f32 / scale,
            offset_y: bounds.top as f32 / scale,
            width: width as f32 / scale,
            height: height as f32 / scale,
            source_width: width,
            source_height: height,
        };
        self.run_bytes += byte_count;
        self.run_order.push_back(key);
        self.runs.insert(
            key,
            CachedGlyphRun {
                bitmap: run,
                bytes: byte_count,
                glyphs: glyphs.to_vec(),
            },
        );
        Some(run)
    }

    pub(in crate::windows_app) unsafe fn get_or_create(
        &mut self,
        id: u32,
        image: &DecodedImage,
        tint: Option<[u8; 4]>,
        dc: Hdc,
    ) -> Hbitmap {
        let key = (id, tint);
        if let Some(bitmap) = self.glyphs.get(&key) {
            return *bitmap;
        }
        let pixels = if let Some(tint) = tint {
            tint_mask(image, tint)
        } else {
            image.bgra.clone()
        };
        let info = bitmap_info(image);
        let mut destination = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut destination, null_mut(), 0);
        if bitmap.is_null() || destination.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            return null_mut();
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), destination.cast(), pixels.len());
        self.glyphs.insert(key, bitmap);
        bitmap
    }

    pub(in crate::windows_app) unsafe fn clear(&mut self) {
        for bitmap in self
            .glyphs
            .drain()
            .map(|(_, bitmap)| bitmap)
            .chain(self.runs.drain().map(|(_, run)| run.bitmap.bitmap))
        {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
        }
        self.run_order.clear();
        self.run_bytes = 0;
    }
}

impl Drop for GlyphBitmaps {
    fn drop(&mut self) {
        unsafe { self.clear() }
    }
}

#[derive(Clone, Copy)]
struct PixelBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

fn pixel_bounds(
    glyphs: &[PositionedGlyph],
    resources: &HashMap<u32, PresentedGlyphRaster>,
    scale: f32,
) -> Option<PixelBounds> {
    let first = glyphs.first()?;
    let first_resource = glyph_resource(first, resources)?;
    let left = scaled_coordinate(first.x, scale)?;
    let top = scaled_coordinate(first.y, scale)?;
    let mut bounds = PixelBounds {
        left,
        top,
        right: left.checked_add(i32::try_from(first_resource.image.width).ok()?)?,
        bottom: top.checked_add(i32::try_from(first_resource.image.height).ok()?)?,
    };
    for glyph in &glyphs[1..] {
        let resource = glyph_resource(glyph, resources)?;
        let left = scaled_coordinate(glyph.x, scale)?;
        let top = scaled_coordinate(glyph.y, scale)?;
        bounds.left = bounds.left.min(left);
        bounds.top = bounds.top.min(top);
        bounds.right = bounds
            .right
            .max(left.checked_add(i32::try_from(resource.image.width).ok()?)?);
        bounds.bottom = bounds
            .bottom
            .max(top.checked_add(i32::try_from(resource.image.height).ok()?)?);
    }
    Some(bounds)
}

fn glyph_resource<'a>(
    glyph: &PositionedGlyph,
    resources: &'a HashMap<u32, PresentedGlyphRaster>,
) -> Option<&'a PresentedGlyphRaster> {
    let resource = resources.get(&glyph.raster_id)?;
    (resource.color == glyph.color).then_some(resource)
}

fn scaled_coordinate(value: f32, scale: f32) -> Option<i32> {
    let value = value * scale;
    (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
        .then_some(value.round() as i32)
}

fn composite_glyph(
    destination: &mut [u8],
    destination_width: u32,
    destination_height: u32,
    left: i32,
    top: i32,
    resource: &PresentedGlyphRaster,
    tint_table: &[[u8; 4]; 256],
) -> Option<()> {
    let source_width = usize::try_from(resource.image.width).ok()?;
    let source_height = usize::try_from(resource.image.height).ok()?;
    let destination_width = usize::try_from(destination_width).ok()?;
    let destination_height = usize::try_from(destination_height).ok()?;
    let left = usize::try_from(left).ok()?;
    let top = usize::try_from(top).ok()?;
    if left.checked_add(source_width)? > destination_width
        || top.checked_add(source_height)? > destination_height
    {
        return None;
    }
    for source_y in 0..source_height {
        let row_bytes = source_width.checked_mul(4)?;
        let source_start = source_y.checked_mul(row_bytes)?;
        let source_end = source_start.checked_add(row_bytes)?;
        let destination_start = top
            .checked_add(source_y)?
            .checked_mul(destination_width)?
            .checked_add(left)?
            .checked_mul(4)?;
        let destination_end = destination_start.checked_add(row_bytes)?;
        let source_row = resource.image.bgra.get(source_start..source_end)?;
        let destination_row = destination.get_mut(destination_start..destination_end)?;
        for (source, destination) in source_row
            .chunks_exact(4)
            .zip(destination_row.chunks_exact_mut(4))
        {
            let pixel = if resource.color {
                [source[0], source[1], source[2], source[3]]
            } else {
                tint_table[usize::from(source[3])]
            };
            if destination[3] == 0 {
                destination.copy_from_slice(&pixel);
            } else if pixel[3] != 0 {
                source_over(destination, pixel);
            }
        }
    }
    Some(())
}

fn tint_pixel(mask_alpha: u8, tint: [u8; 4]) -> [u8; 4] {
    let alpha = u16::from(mask_alpha) * u16::from(tint[3]) / 255;
    [
        (u16::from(tint[2]) * alpha / 255) as u8,
        (u16::from(tint[1]) * alpha / 255) as u8,
        (u16::from(tint[0]) * alpha / 255) as u8,
        alpha as u8,
    ]
}

fn mask_tint_table(tint: [u8; 4]) -> [[u8; 4]; 256] {
    std::array::from_fn(|alpha| tint_pixel(alpha as u8, tint))
}

fn source_over(destination: &mut [u8], source: [u8; 4]) {
    let inverse_alpha = 255_u16 - u16::from(source[3]);
    for channel in 0..3 {
        destination[channel] = u16::from(source[channel])
            .saturating_add(u16::from(destination[channel]) * inverse_alpha / 255)
            .min(255) as u8;
    }
    destination[3] = u16::from(source[3])
        .saturating_add(u16::from(destination[3]) * inverse_alpha / 255)
        .min(255) as u8;
}

fn tint_mask(image: &DecodedImage, tint: [u8; 4]) -> Vec<u8> {
    let mut tinted = Vec::with_capacity(image.bgra.len());
    let table = mask_tint_table(tint);
    for pixel in image.bgra.chunks_exact(4) {
        tinted.extend_from_slice(&table[usize::from(pixel[3])]);
    }
    tinted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(id: u32, color: bool) -> PresentedGlyphRaster {
        PresentedGlyphRaster {
            id,
            image: DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![255; 4],
            },
            color,
        }
    }

    fn glyph(id: u32, color: bool) -> PositionedGlyph {
        PositionedGlyph {
            raster_id: id,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            color,
        }
    }

    #[test]
    fn source_over_composites_premultiplied_pixels() {
        let mut destination = [0, 0, 128, 128];
        source_over(&mut destination, [0, 128, 0, 128]);

        assert_eq!(destination, [0, 128, 63, 191]);
    }

    #[test]
    fn tint_pixel_preserves_premultiplied_alpha() {
        assert_eq!(tint_pixel(128, [10, 20, 30, 128]), [7, 5, 2, 64]);
    }

    #[test]
    fn unknown_or_mismatched_glyph_resources_fail_closed() {
        let resources = HashMap::from([(1, raster(1, false))]);

        assert!(pixel_bounds(&[glyph(2, false)], &resources, 1.0).is_none());
        assert!(pixel_bounds(&[glyph(1, true)], &resources, 1.0).is_none());
        assert!(pixel_bounds(&[glyph(1, false)], &resources, 1.0).is_some());
    }
}
