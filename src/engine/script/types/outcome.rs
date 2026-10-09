//! Author-task results and fail-closed rendering request coalescing.
use super::*;
use crate::engine::scheduler::RenderScope;

#[derive(Debug, Default, Clone)]
pub struct ScriptOutcome {
    pub executed: usize,
    pub mutation_count: usize,
    pub errors: Vec<String>,
    pub console: Vec<String>,
    pub diagnostics: Vec<String>,
    pub navigation_url: Option<String>,
    pub navigation_options: crate::navigation::request::NavigationOptions,
    /// Latest script-requested vertical viewport offset, in CSS pixels.
    pub viewport_scroll_y: Option<f32>,
    /// Unconsumed wheel default, applied relative to the browser's current animation target.
    pub viewport_wheel_delta_y: f32,
    pub history_actions: Vec<ScriptHistoryAction>,
    pub cookie_updates: Vec<String>,
    pub storage_updates: Vec<StorageWrite>,
    pub policy_updates: Vec<ScriptPolicyUpdate>,
    pub storage_event_receipts: Vec<(crate::storage::StorageAreaKind, u64)>,
    pub fetch_actions: Vec<ScriptFetchAction>,
    pub websocket_actions: Vec<ScriptWebSocketAction>,
    pub database_actions: Vec<ScriptDatabaseAction>,
    pub speech_actions: Vec<ScriptSpeechAction>,
    pub notification_actions: Vec<ScriptNotificationAction>,
    pub protocol_handler_actions: Vec<ScriptProtocolHandlerAction>,
    pub clipboard_actions: Vec<ScriptClipboardAction>,
    pub file_picker_actions: Vec<ScriptFilePickerAction>,
    pub selection_actions: Vec<ScriptSelectionAction>,
    pub permission_actions: Vec<crate::renderer_protocol::PermissionRequest>,
    pub geolocation_actions: Vec<ScriptGeolocationAction>,
    pub media_device_actions: Vec<ScriptMediaDeviceAction>,
    pub sensor_actions: Vec<ScriptSensorAction>,
    pub broadcast_actions: Vec<ScriptBroadcastAction>,
    pub worker_actions: Vec<ScriptWorkerAction>,
    pub fullscreen_actions: Vec<ScriptFullscreenAction>,
    pub wake_lock_actions: Vec<ScriptWakeLockAction>,
    pub pointer_lock_actions: Vec<ScriptPointerLockAction>,
    pub media_actions: Vec<ScriptMediaAction>,
    pub graph_audio_actions: Vec<ScriptGraphAudioAction>,
    pub font_actions: Vec<ScriptFontAction>,
    pub runtime_stopped: bool,
    pub render_requested: bool,
    /// Native scheduling evidence, not an author-controlled optimization hint.
    /// A requested render with no scope always requires the ordinary full path.
    pub render_scope: Option<RenderScope>,
    pub invalidation: RenderInvalidation,
}

impl ScriptOutcome {
    pub(crate) fn request_full_render(&mut self) {
        self.render_requested = true;
        self.render_scope = Some(RenderScope::Layout);
    }

    pub(crate) fn merge_render_request(&mut self, requested: bool, scope: Option<RenderScope>) {
        if !requested {
            return;
        }
        self.render_scope = Some(
            if scope == Some(RenderScope::SurfacePixels)
                && (!self.render_requested || self.render_scope == Some(RenderScope::SurfacePixels))
            {
                RenderScope::SurfacePixels
            } else {
                RenderScope::Layout
            },
        );
        self.render_requested = true;
    }

    pub(crate) fn is_surface_repaint(&self) -> bool {
        self.render_requested
            && self.render_scope == Some(RenderScope::SurfacePixels)
            && self.mutation_count == 0
            && self.invalidation == RenderInvalidation::default()
            && self.has_no_geometry_actions()
    }

    pub(crate) fn is_style_only_refresh(&self) -> bool {
        self.render_requested
            && self.invalidation.impact.is_style_only()
            && self.has_no_geometry_actions()
    }

    fn has_no_geometry_actions(&self) -> bool {
        !self.runtime_stopped
            && self.navigation_url.is_none()
            && self.viewport_scroll_y.is_none()
            && self.viewport_wheel_delta_y == 0.0
            && self.history_actions.is_empty()
            && self.selection_actions.is_empty()
            && self.font_actions.is_empty()
            && self.media_actions.is_empty()
            && self.fullscreen_actions.is_empty()
    }
}

#[cfg(test)]
mod tests;
