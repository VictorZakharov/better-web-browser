use super::*;
use better_web_browser::engine::RectF;
use std::collections::HashMap;

impl Compositor {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_background(
        &mut self,
        images: &HashMap<String, DecodedImage>,
        clip_rect: RectF,
        tile_rect: RectF,
        url: &str,
        repeat_x: bool,
        repeat_y: bool,
        scale: f32,
        scroll: i32,
    ) -> Result<(), String> {
        let Some(image) = images.get(url) else {
            return Ok(());
        };
        let bitmap = self.images.get(&self.device, url, image, None)?;
        let clip = primitives::rect(clip_rect, scroll, scale);
        let tile = primitives::rect(tile_rect, scroll, scale);
        let width = tile.right - tile.left;
        let height = tile.bottom - tile.top;
        if width <= 0.0 || height <= 0.0 {
            return Ok(());
        }
        let left = if repeat_x {
            tile.left - ((tile.left - clip.left) / width).ceil() * width
        } else {
            tile.left
        };
        let top = if repeat_y {
            tile.top - ((tile.top - clip.top) / height).ceil() * height
        } else {
            tile.top
        };
        let cols = if repeat_x {
            ((clip.right - left) / width).ceil().max(0.0) as usize
        } else {
            1
        };
        let rows = if repeat_y {
            ((clip.bottom - top) / height).ceil().max(0.0) as usize
        } else {
            1
        };
        if cols.saturating_mul(rows) > 4096 {
            return Err("GPU background tile count exceeds limit".into());
        }
        unsafe {
            self.device
                .d2d
                .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        }
        for y in 0..rows {
            for x in 0..cols {
                let left = left + x as f32 * width;
                let top = top + y as f32 * height;
                primitives::draw(
                    &self.device,
                    &bitmap,
                    D2D_RECT_F {
                        left,
                        top,
                        right: left + width,
                        bottom: top + height,
                    },
                );
            }
        }
        unsafe {
            self.device.d2d.PopAxisAlignedClip();
        }
        Ok(())
    }
}
