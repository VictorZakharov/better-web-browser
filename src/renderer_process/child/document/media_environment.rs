//! Synchronization of CSS and JavaScript media state for one isolated document.

use super::*;
use crate::renderer_protocol::PresentedViewport;

impl DocumentRuntime {
    pub(super) fn apply_media_environment(&mut self, viewport: PresentedViewport) {
        self.prefers_dark_color_scheme = viewport.prefers_dark_color_scheme;
        let environment = MediaEnvironment::new(
            viewport.style_width,
            viewport.height,
            viewport.dpi as f32 / 96.0,
            viewport.prefers_dark_color_scheme,
        );
        self.page.set_media_environment(environment);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_media_environment(environment);
        }
    }
}
