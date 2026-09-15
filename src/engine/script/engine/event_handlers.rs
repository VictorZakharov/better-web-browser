//! Compile HTML FunctionBody source with native object-environment scopes, outside bootstrap's
//! private lexical environment. V8's context extensions implement the specified with semantics.
pub(super) fn compile(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut return_value: v8::ReturnValue,
) {
    let (Ok(body), Ok(name), Ok(extensions), Ok(url), Ok(global)) = (
        v8::Local::<v8::String>::try_from(arguments.get(1)),
        v8::Local::<v8::String>::try_from(arguments.get(2)),
        v8::Local::<v8::Array>::try_from(arguments.get(3)),
        v8::Local::<v8::String>::try_from(arguments.get(4)),
        v8::Local::<v8::Object>::try_from(arguments.get(5)),
    ) else {
        return;
    };
    if extensions.length() > 3 {
        return;
    }
    let Some(context) = global.get_creation_context(scope) else {
        return;
    };
    let scope = &mut v8::ContextScope::new(scope, context);
    let mut scopes = Vec::new();
    for index in 0..extensions.length() {
        let Some(value) = extensions.get_index(scope, index) else {
            return;
        };
        let Ok(object) = v8::Local::<v8::Object>::try_from(value) else {
            return;
        };
        scopes.push(object);
    }
    let parameters: &[&str] = if arguments.get(6).is_true() {
        &["event", "source", "lineno", "colno", "error"]
    } else {
        &["event"]
    };
    let Some(parameters) = parameters
        .iter()
        .map(|name| v8::String::new(scope, name))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let origin = v8::ScriptOrigin::new(
        scope,
        url.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    let mut source = v8::script_compiler::Source::new(body, Some(&origin));
    if let Some(function) = v8::script_compiler::compile_function(
        scope,
        &mut source,
        &parameters,
        &scopes,
        v8::script_compiler::CompileOptions::NoCompileOptions,
        v8::script_compiler::NoCacheReason::NoReason,
    ) {
        function.set_name(name);
        return_value.set(function.into());
    }
}
