//! HTML cross-origin capabilities. Never read author properties on a foreign global.
use super::{frames, messaging, node_wrappers};
mod location;
pub(super) use location::guard as guard_navigation;

const NAMES: &[&str] = &[
    "window",
    "self",
    "location",
    "close",
    "closed",
    "focus",
    "blur",
    "frames",
    "length",
    "top",
    "opener",
    "parent",
    "postMessage",
];

pub(super) fn install(scope: &mut v8::PinScope) -> Option<()> {
    let context = scope.get_current_context();
    let window = context.global(scope);
    let mut relations = Vec::new();
    for name in ["parent", "top"] {
        let key = v8::String::new(scope, name)?;
        relations.push(window.get(scope, key.into())?);
    }
    let relations = v8::Array::new_with_elements(scope, &relations);
    let key = private(scope, "Breeze.WindowRelations")?;
    window.set_private(scope, key, relations.into())?;
    install_hook(scope, window, window, false)?;
    location::install(scope, window)
}

fn private<'s>(scope: &mut v8::PinScope<'s, '_>, name: &str) -> Option<v8::Local<'s, v8::Private>> {
    let name = v8::String::new(scope, name)?;
    Some(v8::Private::for_api(scope, Some(name)))
}

fn install_hook<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    window: v8::Local<'s, v8::Object>,
    location: bool,
) -> Option<()> {
    let kind = v8::Boolean::new(scope, location);
    let data = v8::Array::new_with_elements(scope, &[window.into(), kind.into()]);
    let hook = v8::Function::builder(intercept)
        .data(data.into())
        .build(scope)?;
    let key = private(scope, "Breeze.CrossOriginHook")?;
    object.set_private(scope, key, hook.into())?;
    Some(())
}

fn intercept(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let data = v8::Local::new(scope, args.data());
    let Some((target, location)) = target_data(scope, data) else {
        return;
    };
    let Ok(caller) = v8::Local::<v8::Object>::try_from(args.get(2)) else {
        return;
    };
    let Some(caller) = caller.get_creation_context(scope) else {
        return;
    };
    let scope = &mut v8::ContextScope::new(scope, caller);
    let operation = args.get(0).to_rust_string_lossy(scope);
    let property = v8::Local::new(scope, args.get(1));
    let name = property.to_rust_string_lossy(scope);
    let value = match operation.as_str() {
        "get" => read(scope, target, location, property),
        "descriptor" => descriptor(scope, target, location, property),
        "keys" => keys(scope, target, location),
        "set" if (!location && name == "location") || (location && name == "href") => {
            location::navigate(scope, target, args.get(3), false);
            Some(v8::undefined(scope).into())
        }
        _ => {
            deny(scope);
            None
        }
    };
    if let Some(value) = value {
        result.set(value);
    }
}

fn target_data<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Option<(v8::Local<'s, v8::Object>, bool)> {
    let data = v8::Local::<v8::Array>::try_from(value).ok()?;
    let target = v8::Local::<v8::Object>::try_from(data.get_index(scope, 0)?).ok()?;
    Some((target, data.get_index(scope, 1)?.boolean_value(scope)))
}

fn fallback(scope: &mut v8::PinScope, key: v8::Local<v8::Value>) -> bool {
    if key.is_string() {
        return key.to_rust_string_lossy(scope) == "then";
    }
    [
        v8::Symbol::get_to_string_tag(scope),
        v8::Symbol::get_has_instance(scope),
        v8::Symbol::get_is_concat_spreadable(scope),
    ]
    .into_iter()
    .any(|symbol| key.strict_equals(symbol.into()))
}

fn method(name: &str, location: bool) -> bool {
    if location {
        name == "replace"
    } else {
        matches!(name, "postMessage" | "close" | "focus" | "blur")
    }
}

fn known<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    location: bool,
    key: v8::Local<v8::Value>,
) -> bool {
    if fallback(scope, key) {
        return true;
    }
    if !key.is_string() {
        return false;
    }
    let name = key.to_rust_string_lossy(scope);
    if location {
        matches!(name.as_str(), "href" | "replace")
    } else {
        NAMES.contains(&name.as_str())
            || index(&name).is_some_and(|index| index < children(scope, target).len())
    }
}

fn index(name: &str) -> Option<usize> {
    let index: u32 = name.parse().ok()?;
    (index != u32::MAX && index.to_string() == name).then_some(index as usize)
}

fn children<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
) -> Vec<v8::Local<'s, v8::Object>> {
    let Some(context) = target.get_creation_context(scope) else {
        return Vec::new();
    };
    frames::child_windows(scope, context)
}

fn read<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    location: bool,
    key: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Value>> {
    if !known(scope, target, location, key)
        || (location && key.to_rust_string_lossy(scope) == "href")
    {
        deny(scope);
        return None;
    }
    if fallback(scope, key) {
        return Some(v8::undefined(scope).into());
    }
    let name = key.to_rust_string_lossy(scope);
    if method(&name, location) {
        let desc = descriptor(scope, target, location, key)?;
        let desc = v8::Local::<v8::Object>::try_from(desc).ok()?;
        let value = v8::String::new(scope, "value")?;
        return desc.get(scope, value.into());
    }
    if let Some(index) = index(&name) {
        return children(scope, target).get(index).copied().map(Into::into);
    }
    let context = target.get_creation_context(scope)?;
    match name.as_str() {
        "window" | "self" | "frames" => Some(target.into()),
        "closed" => Some(v8::Boolean::new(scope, !frames::active(context)).into()),
        "length" => {
            let count = children(scope, target).len();
            Some(v8::Integer::new(scope, count as i32).into())
        }
        "opener" => Some(v8::null(scope).into()),
        "parent" | "top" => {
            let scope = &mut v8::ContextScope::new(scope, context);
            let key = private(scope, "Breeze.WindowRelations")?;
            let value = target.get_private(scope, key)?;
            v8::Local::<v8::Array>::try_from(value)
                .ok()?
                .get_index(scope, u32::from(name == "top"))
        }
        "location" => location::object(scope, target).map(Into::into),
        _ => {
            deny(scope);
            None
        }
    }
}

fn function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    key: v8::Local<'s, v8::Value>,
    action: &str,
) -> Option<v8::Local<'s, v8::Function>> {
    let action = v8::String::new(scope, action)?;
    let data = v8::Array::new_with_elements(scope, &[target.into(), key, action.into()]);
    v8::Function::builder(call)
        .data(data.into())
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope)
}

fn call(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let Ok(data) = v8::Local::<v8::Array>::try_from(args.data()) else {
        return;
    };
    let Some(target) = data
        .get_index(scope, 0)
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        return;
    };
    let Some(key) = data.get_index(scope, 1) else {
        return;
    };
    let Some(action) = data.get_index(scope, 2) else {
        return;
    };
    let name = key.to_rust_string_lossy(scope);
    match action.to_rust_string_lossy(scope).as_str() {
        "get" => {
            if let Some(value) = read(scope, target, false, key) {
                result.set(value);
            }
        }
        "set" => location::navigate(scope, target, args.get(0), false),
        "hash" => location::component(scope, target, "hash", args.get(0)),
        _ => match name.as_str() {
            "postMessage" => messaging::post_message_to(scope, args, target),
            "replace" => location::navigate(scope, target, args.get(0), true),
            "assign" => location::navigate(scope, target, args.get(0), false),
            "reload" => location::component(scope, target, "reload", args.get(0)),
            // Child navigables are not script-closable; focus/blur do not steal OS focus.
            "close" | "focus" | "blur" => {}
            _ => deny(scope),
        },
    }
}

fn descriptor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    location: bool,
    key: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Value>> {
    if !known(scope, target, location, key) {
        deny(scope);
        return None;
    }
    let name = key.to_rust_string_lossy(scope);
    let context = target.get_creation_context(scope)?;
    let id = node_wrappers::host(context)?.borrow().document.id();
    let cache_key = v8::String::new(scope, &format!("{id:?}:{location}:{name}"))?;
    let caller = scope.get_current_context().global(scope);
    let private_key = private(scope, "Breeze.CrossOriginDescriptors")?;
    let cached = caller.get_private(scope, private_key)?;
    let cache = if let Ok(cache) = v8::Local::<v8::Map>::try_from(cached) {
        cache
    } else {
        let cache = v8::Map::new(scope);
        caller.set_private(scope, private_key, cache.into())?;
        cache
    };
    let cached = cache.get(scope, cache_key.into())?;
    if cached.is_object() {
        return Some(cached);
    }
    let desc = v8::Object::new(scope);
    let null = v8::null(scope);
    desc.set_prototype(scope, null.into())?;
    let yes = v8::Boolean::new(scope, true).into();
    let no = v8::Boolean::new(scope, false).into();
    put(scope, desc, "configurable", yes)?;
    put(scope, desc, "enumerable", no)?;
    if method(&name, location) {
        let value = function(scope, target, key, "call")?;
        put(scope, desc, "value", value.into())?;
        put(scope, desc, "writable", no)?;
    } else if fallback(scope, key) || index(&name).is_some() {
        let value = read(scope, target, location, key)?;
        put(scope, desc, "value", value)?;
        put(scope, desc, "writable", no)?;
    } else {
        let get = if location {
            v8::undefined(scope).into()
        } else {
            function(scope, target, key, "get")?.into()
        };
        put(scope, desc, "get", get)?;
        let set = if name == "location" || name == "href" {
            function(scope, target, key, "set")?.into()
        } else {
            v8::undefined(scope).into()
        };
        put(scope, desc, "set", set)?;
    }
    cache.set(scope, cache_key.into(), desc.into())?;
    Some(desc.into())
}

fn keys<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    location: bool,
) -> Option<v8::Local<'s, v8::Value>> {
    let mut keys = Vec::new();
    if !location {
        for index in 0..children(scope, target).len() {
            keys.push(v8::String::new(scope, &index.to_string())?.into());
        }
    }
    for name in if location {
        &["href", "replace"][..]
    } else {
        NAMES
    } {
        keys.push(v8::String::new(scope, name)?.into());
    }
    keys.push(v8::String::new(scope, "then")?.into());
    keys.push(v8::Symbol::get_to_string_tag(scope).into());
    keys.push(v8::Symbol::get_has_instance(scope).into());
    keys.push(v8::Symbol::get_is_concat_spreadable(scope).into());
    Some(v8::Array::new_with_elements(scope, &keys).into())
}

fn put(
    scope: &mut v8::PinScope,
    object: v8::Local<v8::Object>,
    name: &str,
    value: v8::Local<v8::Value>,
) -> Option<()> {
    let key = v8::String::new(scope, name)?;
    object.set(scope, key.into(), value)?;
    Some(())
}
fn deny(scope: &mut v8::PinScope) {
    messaging::throw_named(
        scope,
        "SecurityError",
        "Blocked cross-origin Window or Location access",
    );
}
