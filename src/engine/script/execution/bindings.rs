//! Browser binding initialization, including private embedder callback capture.
use super::*;

pub(super) fn initialize(context: &mut Context) -> Result<(), String> {
    context
        .initialize_iframe_realm(IFRAME_REALM_BOOTSTRAP)
        .map_err(|error| format!("initialize iframe browser bindings: {error}"))?;
    context
        .eval(Source::from_bytes(
            super::super::bootstrap::BROWSER_BOOTSTRAP,
        ))
        .map_err(|error| format!("initialize browser bindings: {error}"))?;
    context
        .capture_hook("__dispatchStorageEvent")
        .map_err(|error| format!("capture storage event dispatcher: {error}"))
}
const IFRAME_REALM_BOOTSTRAP: &str = r#"
globalThis.window = globalThis;
globalThis.self = globalThis;
if (typeof String.prototype.substr !== 'function') {
    Object.defineProperty(String.prototype, 'substr', {
        configurable: true,
        writable: true,
        value(start, length) {
            const string = String(this);
            const size = string.length;
            let from = Number(start) || 0;
            from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
            if (length === undefined) return string.slice(from);
            let count = Number(length);
            if (Number.isNaN(count) || count <= 0) return '';
            if (count !== Infinity) count = Math.floor(count);
            return string.slice(from, Math.min(from + count, size));
        }
    });
}
"#;
