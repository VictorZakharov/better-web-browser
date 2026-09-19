//! Initial child navigables: a distinct document, global and DOM wrapper cache per iframe.
use super::bridge::{HostBridge, install_host_call};
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::script::{bootstrap, host_state::HostState, module_loader::WebModuleLoader};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

#[derive(Default)]
pub(super) struct FrameTree {
    children: RefCell<HashMap<NodeId, ChildRealm>>,
}

struct ChildRealm {
    context: v8::Global<v8::Context>,
    _storage_dispatch: v8::Global<v8::Function>,
    host: Rc<RefCell<HostState>>,
    parent_document: NodeId,
    active: Rc<Cell<bool>>,
}

struct FrameLink {
    tree: Weak<FrameTree>,
    active: Rc<Cell<bool>>,
}
// Retained DOM objects keep their old document usable after the embedding element is removed.
// This slot contains no V8 handles, so it cannot form a persistent-handle/context cycle.
struct DocumentLifetime {
    _host: Rc<RefCell<HostState>>,
}

pub(super) fn register(context: v8::Local<v8::Context>, tree: &Rc<FrameTree>) {
    context.set_slot(Rc::new(FrameLink {
        tree: Rc::downgrade(tree),
        active: Rc::new(Cell::new(true)),
    }));
}

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    result.set(v8::null(scope).into());
    let parent = scope.get_current_context();
    if operation == "frameActive" {
        result.set(
            v8::Boolean::new(
                scope,
                parent
                    .get_slot::<FrameLink>()
                    .is_some_and(|link| link.active.get()),
            )
            .into(),
        );
        return;
    }
    let Some(tree) = parent
        .get_slot::<FrameLink>()
        .filter(|link| link.active.get())
        .and_then(|link| link.tree.upgrade())
    else {
        return;
    };
    let Some(bridge) = parent.get_slot::<HostBridge>() else {
        return;
    };
    let HostBridge::Document(host) = &*bridge else {
        return;
    };
    let Some(host) = host.upgrade() else { return };
    let Some(id) = arguments.get(1).uint32_value(scope) else {
        return;
    };
    let (node, document) = {
        let state = host.borrow();
        (state.nodes.get(&id).cloned(), state.document.id())
    };
    let Some(node) = node.filter(|node| node.tag_name() == Some("iframe")) else {
        return;
    };
    if operation == "discardFrame" {
        discard(&tree, node.id());
        return;
    }
    if !connected_to(&node, document) {
        discard(&tree, node.id());
        return;
    }
    let cached = tree
        .children
        .borrow()
        .get(&node.id())
        .map(|child| child.context.clone());
    let context = match cached {
        Some(context) => context,
        None => {
            // Bound native contexts independently of the much larger DOM-node budget.
            if tree.children.borrow().len() >= 256 {
                if let Some(message) =
                    v8::String::new(scope, "The document's iframe context limit was reached")
                {
                    let exception = v8::Exception::range_error(scope, message);
                    scope.throw_exception(exception);
                }
                return;
            }
            let element = v8::Local::new(scope, arguments.get(2));
            let Some(child) = create(scope, parent, &node, element, &tree, document) else {
                return;
            };
            let context = child.context.clone();
            tree.children.borrow_mut().insert(node.id(), child);
            context
        }
    };
    let child = v8::Local::new(scope, context);
    if operation == "frameWindow" {
        result.set(child.global(scope).into());
    } else if child.get_security_token(scope) == parent.get_security_token(scope) {
        let child_scope = &mut v8::ContextScope::new(scope, child);
        if let Some(name) = v8::String::new(child_scope, "document")
            && let Some(document) = child.global(child_scope).get(child_scope, name.into())
        {
            result.set(document);
        }
    }
}

fn connected_to(node: &NodeRef, document: NodeId) -> bool {
    let mut current = Some(node.clone());
    while let Some(node) = current {
        if node.id() == document {
            return true;
        }
        current = node.parent().or_else(|| node.shadow_host());
    }
    false
}

fn discard(tree: &FrameTree, id: NodeId) {
    let Some(child) = tree.children.borrow_mut().remove(&id) else {
        return;
    };
    let document = child.host.borrow().document.id();
    child.active.set(false);
    let descendants: Vec<_> = tree
        .children
        .borrow()
        .iter()
        .filter_map(|(id, child)| (child.parent_document == document).then_some(*id))
        .collect();
    for id in descendants {
        discard(tree, id);
    }
}

fn create<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    parent: v8::Local<'s, v8::Context>,
    node: &NodeRef,
    element: v8::Local<'s, v8::Value>,
    tree: &Rc<FrameTree>,
    parent_document: NodeId,
) -> Option<ChildRealm> {
    // HTML creates the initial about:blank document synchronously with connection.
    // Its origin is inherited unless the iframe's sandbox forces a unique opaque origin.
    // https://html.spec.whatwg.org/multipage/document-sequences.html#creating-a-new-browsing-context
    let document =
        crate::engine::dom::parse("<!doctype html><html><head></head><body></body></html>")
            .document;
    let host = Rc::new(RefCell::new(HostState::new(
        document,
        "about:blank",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    if let Some(bridge) = parent.get_slot::<HostBridge>()
        && let HostBridge::Document(parent_host) = &*bridge
        && let Some(parent_host) = parent_host.upgrade()
    {
        host.borrow_mut().about_base_url = Some(parent_host.borrow().script_base_url());
    }
    let context = v8::Context::new(scope, Default::default());
    let same_origin = node.attr("sandbox").is_none_or(|flags| {
        flags
            .split_ascii_whitespace()
            .any(|flag| flag.eq_ignore_ascii_case("allow-same-origin"))
    });
    if same_origin {
        context.set_security_token(parent.get_security_token(scope));
    }
    context.set_slot(Rc::new(HostBridge::Document(Rc::downgrade(&host))));
    context.set_slot(Rc::new(DocumentLifetime {
        _host: Rc::clone(&host),
    }));
    context.set_slot(Rc::new(super::dynamic_imports::Imports::default()));
    register(context, tree);
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
        let parent_key = v8::String::new(scope, "parent")?;
        context
            .global(scope)
            .set(scope, parent_key.into(), parent.global(scope).into())?;
        context.global(scope).set(scope, top_key.into(), top)?;
        let ready = v8::String::new(scope, "__setDocumentComplete()")?;
        v8::Script::compile(scope, ready, None)?.run(scope)?;
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
    Some(ChildRealm {
        context: v8::Global::new(scope, context),
        _storage_dispatch: storage_dispatch,
        host,
        parent_document,
        active,
    })
}
