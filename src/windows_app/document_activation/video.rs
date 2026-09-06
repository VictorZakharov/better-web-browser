//! Paint-only video updates reuse the current scene's geometry and clipping.
use super::*;
use better_web_browser::renderer_protocol::VideoFrameUpdate;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn activate_video_frame(&mut self, update: VideoFrameUpdate) {
        if !self.navigation.owns_document(update.identity.document)
            || self.renderer_revision != update.identity.revision
        {
            return;
        }
        let key = update.identity.image_key();
        let Some(image) = self.presented_images.get_mut(&key) else {
            return;
        };
        // A changed intrinsic size must go through document layout before being composited.
        if image.width != update.identity.width || image.height != update.identity.height {
            return;
        }
        image.bgra = update.pixels;
        self.image_bitmaps.remove(&key);
        if self.benchmark.is_some() {
            match self.paint_benchmark_frame() {
                Ok(_) => self.benchmark.as_mut().unwrap().video_cadence.painted(),
                Err(error) => self.benchmark.as_mut().unwrap().error = Some(error),
            }
            return;
        }
        if !self.processing_background_tab {
            let mut client = Rect::default();
            GetClientRect(self.window, &mut client);
            client.top = self.toolbar_height();
            client.bottom -= self.status_height();
            InvalidateRect(self.window, &client, 0);
        }
    }
}
