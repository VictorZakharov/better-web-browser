//! Attribute wrappers have JS-backed private state, unlike arena-backed tree nodes.
//! Store the bootstrap's reader privately so cross-realm comparisons don't call author getters.
pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    result.set(v8::null(scope).into());
    let Ok(object) = v8::Local::<v8::Object>::try_from(arguments.get(1)) else {
        return;
    };
    let Some(owner) = object.get_creation_context(scope) else {
        return;
    };
    let caller = scope.get_current_context();
    if owner.get_security_token(scope) != caller.get_security_token(scope) {
        return;
    }
    let Some(name) = v8::String::new(scope, "Breeze.Attr.comparison") else {
        return;
    };
    let key = v8::Private::for_api(scope, Some(name));
    if operation == "bindAttributeWrapper" {
        let Ok(reader) = v8::Local::<v8::Function>::try_from(arguments.get(2)) else {
            return;
        };
        if owner == caller && object.has_private(scope, key) == Some(false) {
            let installed = object.set_private(scope, key, reader.into()) == Some(true);
            result.set(v8::Boolean::new(scope, installed).into());
        }
        return;
    }
    let Some(reader) = object
        .get_private(scope, key)
        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
    else {
        return;
    };
    if let Some(data) = reader.call(scope, object.into(), &[]) {
        result.set(data);
    }
}
