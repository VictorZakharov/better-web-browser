//! Convert actual backend exceptions and allocation failures into host diagnostics.
use super::{JsError, JsErrorKind};

pub(super) fn caught_error(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    action: &str,
) -> JsError {
    let exception = scope.exception();
    let detail = exception
        .and_then(|exception| exception.to_string(scope))
        .map(|message| message.to_rust_string_lossy(scope))
        .unwrap_or_else(|| action.to_string());
    let parameters = exception.and_then(|exception| {
        let object = v8::Local::<v8::Object>::try_from(exception).ok()?;
        let mut candidates = Vec::new();
        if let Some(key) = v8::String::new(scope, "params")
            && let Some(value) = object.get(scope, key.into())
            && let Ok(value) = v8::Local::<v8::Object>::try_from(value)
        {
            candidates.push(value);
        }
        // Closure-library errors retain constructor metadata in args[0] rather than `params`.
        // Reading that one ordinary object also makes wrapped third-party errors actionable.
        if let Some(key) = v8::String::new(scope, "args")
            && let Some(value) = object.get(scope, key.into())
            && let Ok(arguments) = v8::Local::<v8::Array>::try_from(value)
            && let Some(value) = arguments.get_index(scope, 0)
            && let Ok(value) = v8::Local::<v8::Object>::try_from(value)
        {
            candidates.push(value);
        }
        let mut fields = Vec::new();
        for parameters in candidates {
            for name in ["error", "event", "originalStack", "componentStack"] {
                let Some(key) = v8::String::new(scope, name) else {
                    continue;
                };
                let Some(value) = parameters.get(scope, key.into()) else {
                    continue;
                };
                if value.is_null_or_undefined() {
                    continue;
                }
                let Some(value) = value.to_string(scope) else {
                    continue;
                };
                let value = value.to_rust_string_lossy(scope);
                let value = value.chars().take(1_024).collect::<String>();
                if !value.is_empty() {
                    fields.push(format!("{name}={value}"));
                }
            }
        }
        (!fields.is_empty()).then(|| fields.join("; "))
    });
    let detail = parameters.map_or(detail.clone(), |parameters| {
        format!("{detail} [{parameters}]")
    });
    let location = exception.and_then(|exception| {
        let message = v8::Exception::create_message(scope, exception);
        let resource = message
            .get_script_resource_name(scope)?
            .to_string(scope)?
            .to_rust_string_lossy(scope);
        if resource.is_empty() {
            return None;
        }
        let line = message.get_line_number(scope).unwrap_or_default();
        if line == 0 {
            return None;
        }
        let column = message.get_start_column();
        (column != usize::MAX)
            .then(|| format!("{resource}:{line}:{}", column.saturating_add(1)))
            .or_else(|| Some(format!("{resource}:{line}")))
    });
    JsError {
        kind: JsErrorKind::Error,
        message: location.map_or(detail.clone(), |location| format!("{detail} at {location}")),
    }
}

pub(super) fn allocation_error(value: &str) -> JsError {
    JsError {
        kind: JsErrorKind::Range,
        message: format!("V8 could not allocate {value}"),
    }
}
