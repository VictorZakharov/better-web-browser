//! Shared CSS and JavaScript media state for a retained document realm.

use super::*;
use crate::engine::css::media::MediaEnvironment;

impl ScriptRuntime {
    pub(crate) fn set_media_environment(&mut self, environment: MediaEnvironment) {
        let mut host = self.host.borrow_mut();
        host.media_environment = environment;
        host.computed_styles = None;
    }

    pub(crate) fn set_layout_viewport(&mut self, width: f32, height: f32) {
        let mut host = self.host.borrow_mut();
        host.layout_viewport_width = width;
        host.layout_viewport_height = height;
    }

    pub(crate) fn set_quirks_mode(&mut self, quirks_mode: bool) {
        self.host.borrow_mut().quirks_mode = quirks_mode;
    }
}
