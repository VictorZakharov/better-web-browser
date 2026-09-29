//! Script result and input types shared with the renderer and app layers.

pub use super::types::{
    DynamicScriptLoader, DynamicScriptRequest, ScriptBroadcastAction, ScriptCaptionCue,
    ScriptClipboardAction, ScriptFetchOptions, ScriptFontAction, ScriptFullscreenAction,
    ScriptGeolocationAction, ScriptGraphAudioAction, ScriptHistoryAction, ScriptInput, ScriptKind,
    ScriptMediaAction, ScriptMediaCommand, ScriptMediaDeviceAction, ScriptNotificationAction,
    ScriptOutcome, ScriptPointerLockAction, ScriptProtocolHandlerAction, ScriptSelectionAction,
    ScriptSensorAction, ScriptSpeechAction, UserInputEvent, UserInputModifiers, UserInputResult,
};
pub use super::wake_lock_host::ScriptWakeLockAction;
