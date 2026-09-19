//! Location's write-only cross-origin surface and navigation authorization.
use super::*;

pub(super) fn install<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    window: v8::Local<'s, v8::Object>,
) -> Option<()> {
    let key = v8::String::new(scope, "location")?;
    let original = window.get(scope, key.into())?;
    let original = v8::Local::<v8::Object>::try_from(original).ok()?;
    let original_key = private(scope, "Breeze.OriginalLocation")?;
    window.set_private(scope, original_key, original.into())?;
    let template = super::super::v8_api::window_template(scope);
    let location = template.new_instance(scope)?;
    let names = original.get_own_property_names(scope, Default::default())?;
    for index in 0..names.length() {
        let name = v8::Local::<v8::Name>::try_from(names.get_index(scope, index)?).ok()?;
        let desc = original.get_own_property_descriptor(scope, name)?;
        let desc = v8::Local::<v8::Object>::try_from(desc).ok()?;
        let value_key = v8::String::new(scope, "value")?;
        let property = name.to_rust_string_lossy(scope);
        let mut desc = if matches!(property.as_str(), "assign" | "replace" | "reload") {
            let method = function(scope, window, name.into(), "call")?;
            v8::PropertyDescriptor::new_from_value_writable(method.into(), false)
        } else if desc.has_own_property(scope, value_key.into())? {
            v8::PropertyDescriptor::new_from_value_writable(
                desc.get(scope, value_key.into())?,
                true,
            )
        } else {
            let get_key = v8::String::new(scope, "get")?;
            let set_key = v8::String::new(scope, "set")?;
            let set = if property == "href" {
                function(scope, window, name.into(), "set")?.into()
            } else if property == "hash" {
                function(scope, window, name.into(), "hash")?.into()
            } else {
                desc.get(scope, set_key.into())?
            };
            v8::PropertyDescriptor::new_from_get_set(desc.get(scope, get_key.into())?, set)
        };
        desc.set_enumerable(true);
        location.define_property(scope, name, &desc)?;
    }
    install_hook(scope, location, window, true)?;
    let location_key = private(scope, "Breeze.Location")?;
    window.set_private(scope, location_key, location.into())?;
    let get = function(scope, window, key.into(), "get")?;
    let set = function(scope, window, key.into(), "set")?;
    let mut desc = v8::PropertyDescriptor::new_from_get_set(get.into(), set.into());
    desc.set_enumerable(true);
    window.define_property(scope, key.into(), &desc)?;
    Some(())
}

pub(super) fn object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    window: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    let context = window.get_creation_context(scope)?;
    let scope = &mut v8::ContextScope::new(scope, context);
    let key = private(scope, "Breeze.Location")?;
    v8::Local::<v8::Object>::try_from(window.get_private(scope, key)?).ok()
}

pub(super) fn navigate(
    scope: &mut v8::PinScope,
    target: v8::Local<v8::Object>,
    value: v8::Local<v8::Value>,
    replace: bool,
) {
    let Some(source) = super::super::v8_api::incumbent(scope) else {
        return;
    };
    let Some(receiver) = target.get_creation_context(scope) else {
        return;
    };
    if !allowed(scope, source, receiver) {
        deny(scope);
        return;
    }
    let Some(value) = value.to_string(scope) else {
        return;
    };
    let url = value.to_rust_string_lossy(scope);
    if source.get_security_token(scope) != receiver.get_security_token(scope)
        && url
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("javascript:")
    {
        deny(scope);
        return;
    }
    let scope = &mut v8::ContextScope::new(scope, receiver);
    let Some(key) = private(scope, "Breeze.OriginalLocation") else {
        return;
    };
    let Some(original) = target
        .get_private(scope, key)
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        return;
    };
    let Some(key) = v8::String::new(scope, if replace { "replace" } else { "assign" }) else {
        return;
    };
    let Some(function) = original
        .get(scope, key.into())
        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
    else {
        return;
    };
    function.call(scope, original.into(), &[value.into()]);
}

pub(super) fn component(
    scope: &mut v8::PinScope,
    target: v8::Local<v8::Object>,
    name: &str,
    value: v8::Local<v8::Value>,
) {
    let Some(source) = super::super::v8_api::incumbent(scope) else {
        return;
    };
    let Some(receiver) = target.get_creation_context(scope) else {
        return;
    };
    if !allowed(scope, source, receiver) {
        deny(scope);
        return;
    }
    let scope = &mut v8::ContextScope::new(scope, receiver);
    let Some(key) = private(scope, "Breeze.OriginalLocation") else {
        return;
    };
    let Some(original) = target
        .get_private(scope, key)
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        return;
    };
    let Some(key) = v8::String::new(scope, name) else {
        return;
    };
    if name == "hash" {
        original.set(scope, key.into(), value);
    } else if let Some(method) = original
        .get(scope, key.into())
        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
    {
        method.call(scope, original.into(), &[]);
    }
}

fn allowed(
    _scope: &mut v8::PinScope,
    source: v8::Local<v8::Context>,
    target: v8::Local<v8::Context>,
) -> bool {
    if !frames::active(source) || !frames::active(target) {
        return false;
    }
    if source == target {
        return true;
    }
    let Some(source_host) = node_wrappers::host(source) else {
        return false;
    };
    let Some(target_host) = node_wrappers::host(target) else {
        return false;
    };
    let source_host = source_host.borrow();
    let target_host = target_host.borrow();
    let Some(tree) = frames::tree(source) else {
        return false;
    };
    if tree.is_ancestor(source_host.document.id(), target_host.document.id()) {
        return true;
    }
    let ancestor = tree.is_ancestor(target_host.document.id(), source_host.document.id());
    let top = !tree.has_parent(target_host.document.id());
    if source_host.sandbox.sandboxed {
        return ancestor
            && top
            && (source_host.sandbox.top_navigation
                || (source_host.sandbox.top_activation && source_host.user_input_active));
    }
    source_host
        .document_origin
        .is_same_origin(&target_host.document_origin)
        || (ancestor && top && source_host.user_input_active)
}

pub(in crate::engine::script::engine) fn guard(scope: &mut v8::PinScope) -> bool {
    let target = scope.get_current_context();
    let Some(source) = super::super::v8_api::incumbent(scope) else {
        return false;
    };
    if allowed(scope, source, target) {
        true
    } else {
        deny(scope);
        false
    }
}
