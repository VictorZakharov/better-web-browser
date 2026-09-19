//! Creation and replacement of a child Document's realm.
use super::*;
pub(super) fn create<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    parent: v8::Local<'s, v8::Context>,
    node: &NodeRef,
    element: v8::Local<'s, v8::Value>,
    tree: &Rc<FrameTree>,
    parent_document: NodeId,
    replacement: Option<Replacement<'s>>,
) -> Option<ChildRealm> {
    // HTML creates the initial about:blank document synchronously with connection.
    // Its origin is inherited unless the iframe's sandbox forces a unique opaque origin.
    // https://html.spec.whatwg.org/multipage/document-sequences.html#creating-a-new-browsing-context
    let initial = replacement.is_none();
    let document = replacement
        .as_ref()
        .map(|input| input.document.clone())
        .unwrap_or_else(|| {
            crate::engine::dom::parse("<!doctype html><html><head></head><body></body></html>")
                .document
        });
    let url = replacement
        .as_ref()
        .map_or("about:blank", |input| input.url.as_str());
    let host = Rc::new(RefCell::new(HostState::new(
        document,
        url,
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    if let Some(bridge) = parent.get_slot::<HostBridge>()
        && let HostBridge::Document(parent_host) = &*bridge
        && let Some(parent_host) = parent_host.upgrade()
    {
        if url.starts_with("about:") {
            host.borrow_mut().about_base_url = Some(parent_host.borrow().script_base_url());
        }
        host.borrow_mut()
            .share_document_registry(&parent_host.borrow());
        host.borrow_mut().fetch_identifiers = Rc::clone(&parent_host.borrow().fetch_identifiers);
        host.borrow_mut().worker_identifiers = Rc::clone(&parent_host.borrow().worker_identifiers);
        host.borrow_mut().script_bytes = Rc::clone(&parent_host.borrow().script_bytes);
    }
    let global_template = super::super::v8_api::window_template(scope);
    let context = v8::Context::new(
        scope,
        v8::ContextOptions {
            global_template: Some(global_template),
            global_object: replacement.as_ref().map(|input| input.proxy.into()),
            ..Default::default()
        },
    );
    let inherit_origin = url.starts_with("about:");
    let parent_host = super::super::node_wrappers::host(parent)?;
    let sandbox = parent_host
        .borrow()
        .sandbox
        .child(node.attr("sandbox").as_deref());
    host.borrow_mut().sandbox = sandbox;
    host.borrow_mut().embedded = true;
    let parent_origin = parent_host.borrow().document_origin.clone();
    let origin = if sandbox.opaque_origin {
        crate::fetch::Origin::opaque()
    } else if inherit_origin {
        parent_origin.clone()
    } else {
        host.borrow().document_origin.clone()
    };
    let same_origin = origin.is_same_origin(&parent_origin);
    host.borrow_mut().document_origin = origin;
    if same_origin {
        context.set_security_token(parent.get_security_token(scope));
    } else {
        for owner in tree
            .documents
            .borrow()
            .values()
            .filter_map(|owner| owner.to_local(scope))
        {
            if super::super::node_wrappers::host(owner).is_some_and(|other| {
                other
                    .borrow()
                    .document_origin
                    .is_same_origin(&host.borrow().document_origin)
            }) {
                context.set_security_token(owner.get_security_token(scope));
                break;
            }
        }
    }
    if let Some(parent_host) = super::super::node_wrappers::host(parent) {
        let parent_host = parent_host.borrow();
        let mut state = host.borrow_mut();
        if inherit_origin {
            state.fetch_client = parent_host.fetch_client;
            state.policy = parent_host.policy.clone();
            state.inherited_url = Some(
                parent_host
                    .inherited_url
                    .as_ref()
                    .unwrap_or(&parent_host.document_url)
                    .clone(),
            );
        }
    }
    context.set_slot(Rc::new(HostBridge::Document(Rc::downgrade(&host))));
    let opaque = host.borrow().document_origin.serialize() == "null";
    host.borrow_mut().fetch_client.opaque = opaque;
    context.set_slot(Rc::new(DocumentLifetime {
        _host: Rc::clone(&host),
    }));
    context.set_slot(Rc::new(super::super::dynamic_imports::Imports::default()));
    register(context, tree);
    tree.documents
        .borrow_mut()
        .insert(host.borrow().document.id(), v8::Weak::new(scope, context));
    let active = Rc::clone(&context.get_slot::<FrameLink>()?.active);
    let top_key = v8::String::new(scope, "top")?;
    let top = parent.global(scope).get(scope, top_key.into())?;
    let storage_dispatch = {
        let scope = &mut v8::ContextScope::new(scope, context);
        install_host_call(scope, context).ok()?;
        let element_key = v8::String::new(scope, "__frameElement")?;
        let element = if same_origin {
            element
        } else {
            v8::null(scope).into()
        };
        context
            .global(scope)
            .set(scope, element_key.into(), element)?;
        let source = v8::String::new(scope, bootstrap::BROWSER_BOOTSTRAP)?;
        v8::Script::compile(scope, source, None)?.run(scope)?;
        super::super::messaging::install(scope)?;
        let parent_key = v8::String::new(scope, "parent")?;
        context
            .global(scope)
            .set(scope, parent_key.into(), parent.global(scope).into())?;
        context.global(scope).set(scope, top_key.into(), top)?;
        super::super::window_access::install(scope)?;
        if initial {
            host.borrow_mut().document_load =
                crate::engine::script::runtime::document_lifecycle::DocumentLoad::initial_blank();
            let ready = v8::String::new(scope, "__setDocumentComplete()")?;
            v8::Script::compile(scope, ready, None)?.run(scope)?;
        }
        // Match top-level initialization: author code must not obtain the private trusted
        // StorageEvent dispatcher merely by accessing a same-origin child Window.
        let key = v8::String::new(scope, "__dispatchStorageEvent")?;
        let function = context.global(scope).get(scope, key.into())?;
        let function = v8::Local::<v8::Function>::try_from(function).ok()?;
        if context.global(scope).delete(scope, key.into()) != Some(true) {
            return None;
        }
        v8::Global::new(scope, function)
    };
    // Install code-generation restrictions after trusted bootstrap initialization.
    context.set_allow_generation_from_strings(
        !sandbox.scripts_blocked && host.borrow().policy.allows_eval(),
    );
    Some(ChildRealm {
        element: node.clone(),
        attributes: None,
        load_notified: false,
        navigation_pending: false,
        navigation_epoch: 0,
        context: v8::Global::new(scope, context),
        _storage_dispatch: storage_dispatch,
        host,
        parent_document,
        parent_host: Rc::downgrade(&parent_host),
        active,
    })
}
