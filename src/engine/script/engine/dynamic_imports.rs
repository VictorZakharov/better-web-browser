//! Native import() promises and script provenance; no page-visible resolver hooks.
use super::bridge::HostBridge;
use crate::engine::script::{ScriptFetchOptions, ScriptKind};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Origin {
    pub base: String,
    pub options: ScriptFetchOptions,
}

#[derive(Default)]
pub(super) struct Imports {
    pub origins: RefCell<HashMap<String, Origin>>,
    pub resolvers: RefCell<HashMap<u32, v8::Global<v8::PromiseResolver>>>,
}

pub(super) fn request<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_options: v8::Local<'s, v8::Data>,
    resource: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let resolver = v8::PromiseResolver::new(scope)?;
    let promise = resolver.get_promise(scope);
    let context = scope.get_current_context();
    let Some(imports) = context.get_slot::<Imports>() else {
        let message = v8::String::new(scope, "dynamic import is unavailable in this realm")?;
        let error = v8::Exception::type_error(scope, message);
        resolver.reject(scope, error);
        return Some(promise);
    };
    let base = resource.to_rust_string_lossy(scope);
    let specifier = specifier.to_rust_string_lossy(scope);
    let origin = imports.origins.borrow().get(&base).cloned();
    let result = (|| {
        if imports.resolvers.borrow().len() >= crate::limits::MAX_DYNAMIC_SCRIPTS {
            return Err("too many unsettled module imports".into());
        }
        if attributes.length() != 0 {
            return Err("import attributes are not supported".into());
        }
        let bridge = context
            .get_slot::<HostBridge>()
            .ok_or("module host is unavailable")?;
        let HostBridge::Document(host) = &*bridge else {
            return Err("dynamic import in worker realms is not yet supported".into());
        };
        let host = host.upgrade().ok_or("module document is inactive")?;
        let mut host = host.borrow_mut();
        let origin = origin.unwrap_or(Origin {
            base: if base.contains("://") {
                base
            } else {
                host.script_base_url()
            },
            options: ScriptFetchOptions::for_kind(ScriptKind::Module),
        });
        let url =
            crate::engine::script::module_loader::resolve_specifier(&origin.base, &specifier)?;
        let mut options = origin.options;
        // A classic script's no-CORS transport includes credentials, but its module
        // fetch options default to same-origin. Explicit use-credentials stays included.
        if options.mode == crate::fetch::RequestMode::NoCors {
            options.credentials = crate::fetch::CredentialsMode::SameOrigin;
        }
        options.mode = crate::fetch::RequestMode::Cors;
        host.module_jobs.import(url, options)
    })();
    match result {
        Ok(id) => {
            imports
                .resolvers
                .borrow_mut()
                .insert(id, v8::Global::new(scope, resolver));
        }
        Err(error) => {
            let message = v8::String::new(scope, &error)?;
            let error = v8::Exception::type_error(scope, message);
            resolver.reject(scope, error);
        }
    }
    Some(promise)
}

pub(super) fn fulfilled(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _: v8::ReturnValue,
) {
    let data = v8::Local::<v8::Array>::try_from(args.data()).unwrap();
    let resolver = take_resolver(scope, data);
    let namespace = data.get_index(scope, 1).unwrap();
    resolver.resolve(scope, namespace);
}

pub(super) fn rejected(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _: v8::ReturnValue,
) {
    let data = v8::Local::<v8::Array>::try_from(args.data()).unwrap();
    let resolver = take_resolver(scope, data);
    resolver.reject(scope, args.get(0));
}

fn take_resolver<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    data: v8::Local<v8::Array>,
) -> v8::Local<'s, v8::PromiseResolver> {
    // Both callbacks and their data are embedder-created and never exposed to author code.
    let id = data
        .get_index(scope, 0)
        .unwrap()
        .uint32_value(scope)
        .unwrap();
    let imports = scope.get_current_context().get_slot::<Imports>().unwrap();
    let resolver = imports.resolvers.borrow_mut().remove(&id).unwrap();
    v8::Local::new(scope, resolver)
}
