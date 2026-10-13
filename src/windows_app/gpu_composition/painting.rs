use super::*;
use better_web_browser::engine::css::Color;
use std::collections::HashMap;

impl Compositor {
    pub(super) fn paint(
        &mut self,
        layout: &LayoutOutput,
        images: &HashMap<String, DecodedImage>,
        glyphs: &HashMap<u32, better_web_browser::renderer_protocol::PresentedGlyphRaster>,
        scale: f32,
        scroll: i32,
    ) -> Result<bool, String> {
        self.images.next_frame()?;
        unsafe {
            self.device.d2d.SetTarget(&self.target.bitmap);
            self.device
                .d2d
                .SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
            groups::identity(&self.device);
            self.device.d2d.BeginDraw();
            let background = if layout.background.alpha == 0 {
                Color {
                    red: 250,
                    green: 250,
                    blue: 248,
                    alpha: 255,
                }
            } else {
                Color {
                    alpha: 255,
                    ..layout.background
                }
            };
            self.device.d2d.Clear(Some(&primitives::color(background)));
        }
        let mut groups = groups::Groups::new(&self.device, self.target.width, self.target.height);
        let result = (|| {
            for item in &layout.items {
                if !groups.handle(self, item, images, scale, scroll)? {
                    self.paint_leaf(item, images, glyphs, scale, scroll, &groups)?;
                }
            }
            if !groups.balanced() {
                return Err("GPU page paint groups remain open".into());
            }
            Ok::<_, String>(())
        })();
        drop(groups);
        let end = unsafe { self.device.d2d.EndDraw(None, None) }
            .map_err(|error| format!("GPU page composition failed: {error}"));
        result?;
        end?;
        let delivered = self.presentation.present(&self.device, &self.target)?;
        self.inflight.push_back(Fence::submit(&self.device)?);
        self.active |= delivered;
        Ok(delivered)
    }
}
