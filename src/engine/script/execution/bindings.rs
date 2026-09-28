//! Browser binding initialization, including private embedder callback capture.
use super::*;

pub(super) fn initialize(context: &mut Context) -> Result<(), String> {
    context
        .eval(Source::from_bytes(
            super::super::bootstrap::BROWSER_BOOTSTRAP,
        ))
        .map_err(|error| format!("initialize browser bindings: {error}"))?;
    context
        .capture_hook("__trackModulePromise")
        .map_err(|error| format!("capture private module completion hook: {error}"))?;
    for name in [
        "__parserDomChanged",
        "__constructParserElement",
        "__resumeDocumentStream",
        "__setCurrentScript",
        "__dispatchNodeEvent",
        "__queuePolicyViolation",
    ] {
        context
            .capture_hook(name)
            .map_err(|error| format!("capture private {name} hook: {error}"))?;
    }
    context
        .install_window_bindings()
        .map_err(|error| format!("initialize window messaging: {error}"))?;
    context
        .eval(Source::from_bytes(
            "for (const frame of document.querySelectorAll('iframe')) void frame.contentWindow;",
        ))
        .map_err(|error| format!("initialize child documents: {error}"))?;
    context
        .capture_hook("__dispatchStorageEvent")
        .map_err(|error| format!("capture storage event dispatcher: {error}"))?;
    context
        .capture_hook("__dispatchBroadcastChannel")
        .map_err(|error| format!("capture BroadcastChannel dispatcher: {error}"))?;
    context
        .capture_hook("__receiveClipboardUpdate")
        .map_err(|error| format!("capture Clipboard completion hook: {error}"))?;
    context
        .capture_hook("__takeCanvasPresentation")
        .map_err(|error| format!("capture Canvas presentation snapshot: {error}"))?;
    context
        .capture_hook("__setHistoryMetrics")
        .map_err(|error| format!("capture history context updater: {error}"))?;
    context
        .capture_hook("__applyHistoryTraversal")
        .map_err(|error| format!("capture history traversal dispatcher: {error}"))
}
