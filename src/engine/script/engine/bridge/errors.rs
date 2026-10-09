//! Native exceptions use the realm's captured constructors, never author globals.
use super::*;

pub(super) fn throw_error(scope: &mut v8::PinScope, error: JsError) {
    if error.kind == JsErrorKind::Security {
        // History URL rewriting throws a DOMException, not a renamed Error.
        super::super::messaging::throw_named(scope, "SecurityError", &error.message);
        return;
    }
    let Some(message) = v8::String::new(scope, &error.message) else {
        return;
    };
    let exception = match error.kind {
        JsErrorKind::Error => v8::Exception::error(scope, message),
        JsErrorKind::Type => v8::Exception::type_error(scope, message),
        JsErrorKind::Range => v8::Exception::range_error(scope, message),
        JsErrorKind::Security => unreachable!("handled by realm DOMException constructor"),
    };
    scope.throw_exception(exception);
}

pub(super) fn allocation_error(value: &str) -> JsError {
    JsError {
        kind: JsErrorKind::Range,
        message: format!("V8 could not allocate {value}"),
    }
}
