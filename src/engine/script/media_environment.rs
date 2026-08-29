//! Shared CSS and JavaScript media state for a retained document realm.

use super::*;
use crate::engine::css::media::MediaEnvironment;

impl ScriptRuntime {
    pub(crate) fn set_media_environment(&mut self, environment: MediaEnvironment) {
        let mut host = self.host.borrow_mut();
        host.media_environment = environment;
        host.computed_styles = None;
    }
}
