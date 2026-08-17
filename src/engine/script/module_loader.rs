//! URL-based source and module cache for ECMAScript module graphs.

use crate::navigation::resolve_url;
use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::object::JsObject;
use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue, Module, Source};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

#[derive(Debug, Default)]
pub(super) struct WebModuleLoader {
    sources: RefCell<HashMap<String, String>>,
    modules: RefCell<HashMap<String, Module>>,
    missing: RefCell<Vec<String>>,
}

impl WebModuleLoader {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn begin_attempt(&self, url: &str, root: Module) {
        self.modules.borrow_mut().clear();
        self.missing.borrow_mut().clear();
        self.modules.borrow_mut().insert(url.to_string(), root);
    }

    pub(super) fn add_source(&self, url: String, source: String) -> bool {
        self.sources.borrow_mut().insert(url, source).is_none()
    }

    pub(super) fn take_missing(&self) -> Vec<String> {
        std::mem::take(&mut *self.missing.borrow_mut())
    }

    pub(super) fn clear(&self) {
        self.sources.borrow_mut().clear();
        self.modules.borrow_mut().clear();
        self.missing.borrow_mut().clear();
    }
}

impl ModuleLoader for WebModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier = specifier.to_std_string_escaped();
        let url = resolve_specifier(&referrer, &specifier)?;
        if let Some(module) = self.modules.borrow().get(&url) {
            return Ok(module.clone());
        }
        let Some(source_text) = self.sources.borrow().get(&url).cloned() else {
            let mut missing = self.missing.borrow_mut();
            if !missing.contains(&url) {
                missing.push(url.clone());
            }
            return Err(JsNativeError::typ()
                .with_message(format!("module source is not loaded: {url}"))
                .into());
        };
        let module = parse_module(&source_text, &url, &mut context.borrow_mut())?;
        self.modules.borrow_mut().insert(url, module.clone());
        Ok(module)
    }

    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        module: &Module,
        context: &mut Context,
    ) {
        if let Some(path) = module.path() {
            let _ = import_meta.create_data_property(
                boa_engine::js_string!("url"),
                JsValue::from(JsString::from(path.to_string_lossy().as_ref())),
                context,
            );
        }
    }
}

pub(super) fn parse_module(code: &str, url: &str, context: &mut Context) -> JsResult<Module> {
    let mut bytes = code.as_bytes();
    Module::parse(
        Source::from_reader(&mut bytes, Some(Path::new(url))),
        None,
        context,
    )
}

fn resolve_specifier(referrer: &Referrer, specifier: &str) -> JsResult<String> {
    let is_relative =
        specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/');
    let is_absolute = specifier.contains("://");
    if !is_relative && !is_absolute {
        return Err(JsNativeError::typ()
            .with_message(format!("bare module specifier is not mapped: {specifier}"))
            .into());
    }
    let base = referrer
        .path()
        .and_then(Path::to_str)
        .ok_or_else(|| JsNativeError::typ().with_message("module referrer has no URL"))?;
    resolve_url(base, specifier).ok_or_else(|| {
        JsNativeError::typ()
            .with_message(format!(
                "could not resolve module `{specifier}` from `{base}`"
            ))
            .into()
    })
}
