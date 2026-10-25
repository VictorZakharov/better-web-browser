use super::super::paint_primitives::{
    MAX_BACKGROUND_TILES, background_tile_count, background_tiles,
};
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
        if background_tile_count(clip_rect, tile_rect, repeat_x, repeat_y)
            > MAX_BACKGROUND_TILES as u64
        {
            return Err("GPU background tile count exceeds limit".into());
        }
        unsafe {
            self.device
                .d2d
                .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        }
        // Snap each tile like the GDI painter so both paths cover the same device pixels.
        for tile in background_tiles(clip_rect, tile_rect, repeat_x, repeat_y) {
            primitives::draw(&self.device, &bitmap, primitives::rect(tile, scroll, scale));
        }
        unsafe {
            self.device.d2d.PopAxisAlignedClip();
        }
        Ok(())
    }
}
