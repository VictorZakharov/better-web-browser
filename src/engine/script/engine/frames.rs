//! Initial child navigables: a distinct document, global and DOM wrapper cache per iframe.
use super::bridge::{HostBridge, install_host_call};
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::script::{bootstrap, host_state::HostState, module_loader::WebModuleLoader};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
mod creation;
mod navigation;
mod relations;
use creation::create;
pub(in crate::engine::script) use navigation::{FrameNavigation, Replacement};
pub(super) use relations::child_windows;

#[derive(Default)]
pub(super) struct FrameTree {
    children: RefCell<HashMap<NodeId, ChildRealm>>,
    wrappers: Rc<super::node_wrappers::Wrappers>,
    documents: RefCell<HashMap<NodeId, v8::Weak<v8::Context>>>,
    pub(super) messages: super::messaging::Messages,
    pub(super) ports: Rc<super::ports::Ports>,
    pub(super) prefer_port_message: Cell<bool>,
}

struct ChildRealm {
    element: NodeRef,
    attributes: Option<(Option<String>, Option<String>)>,
    load_notified: bool,
    navigation_pending: bool,
    navigation_epoch: u64,
    context: v8::Global<v8::Context>,
    _storage_dispatch: v8::Global<v8::Function>,
    host: Rc<RefCell<HostState>>,
    parent_document: NodeId,
    parent_host: Weak<RefCell<HostState>>,
    active: Rc<Cell<bool>>,
}

pub(super) type RealmSnapshot = (
    NodeId,
    v8::Global<v8::Context>,
    Rc<RefCell<HostState>>,
    v8::Global<v8::Function>,
);

impl FrameTree {
    pub(super) fn child_elements(&self, parent_document: NodeId) -> Vec<(NodeId, NodeId)> {
        self.children
            .borrow()
            .values()
            .filter(|child| child.parent_document == parent_document && child.active.get())
            .map(|child| (child.element.id(), child.host.borrow().document.id()))
            .collect()
    }
    pub(super) fn pending_parents(&self) -> std::collections::HashSet<NodeId> {
        self.children
            .borrow()
            .values()
            .filter(|child| !child.load_notified)
            .map(|child| child.parent_document)
            .collect()
    }

    pub(super) fn ready_load(&self, take: bool) -> Option<(NodeId, NodeRef)> {
        for child in self.children.borrow_mut().values_mut() {
            if child.load_notified
                || child.navigation_pending
                || child.attributes.is_none()
                || !child.host.borrow().document_load.complete()
            {
                continue;
            }
            if take {
                child.load_notified = true;
            }
            return Some((child.parent_document, child.element.clone()));
        }
        None
    }

    pub(super) fn navigation_current(&self, element: NodeId, document: NodeId, epoch: u64) -> bool {
        self.children.borrow().get(&element).is_some_and(|child| {
            child.host.borrow().document.id() == document && child.navigation_epoch == epoch
        })
    }

    pub(super) fn navigation_failed(&self, navigation: &FrameNavigation) {
        let mut children = self.children.borrow_mut();
        if let Some(child) = children.get_mut(&navigation.element)
            && child.host.borrow().document.id() == navigation.document
            && child.navigation_epoch == navigation.epoch
        {
            // Failed child navigations still release the embedding document's load delay.
            // HTML deliberately does not expose a network-error event on iframe elements.
            child.navigation_pending = false;
        }
    }

    pub(super) fn snapshot(&self) -> Vec<RealmSnapshot> {
        self.children
            .borrow()
            .values()
            .map(|child| {
                (
                    child.host.borrow().document.id(),
                    child.context.clone(),
                    Rc::clone(&child.host),
                    child._storage_dispatch.clone(),
                )
            })
            .collect()
    }
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
    context.set_slot(Rc::clone(&tree.wrappers));
    context.set_slot(Rc::new(FrameLink {
        tree: Rc::downgrade(tree),
        active: Rc::new(Cell::new(true)),
    }));
}

pub(super) fn tree(context: v8::Local<v8::Context>) -> Option<Rc<FrameTree>> {
    context.get_slot::<FrameLink>()?.tree.upgrade()
}

pub(super) fn active(context: v8::Local<v8::Context>) -> bool {
    context
        .get_slot::<FrameLink>()
        .is_some_and(|link| link.active.get())
}

pub(super) fn register_document(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
    tree: &FrameTree,
) {
    if let Some(host) = super::node_wrappers::host(context) {
        tree.documents
            .borrow_mut()
            .insert(host.borrow().document.id(), v8::Weak::new(scope, context));
    }
}

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    result.set(v8::null(scope).into());
    let caller = scope.get_current_context();
    let mut parent = caller;
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
        let node = state.nodes.get(&id).cloned();
        let document = node.as_ref().and_then(|node| state.document_for(node));
        (node, document.map(|document| document.id()))
    };
    let Some(node) = node.filter(|node| node.tag_name() == Some("iframe")) else {
        return;
    };
    if operation == "discardFrame" {
        discard(&tree, node.id());
        return;
    }
    let Some(document) = document else { return };
    if let Some(owner) = tree
        .documents
        .borrow()
        .get(&document)
        .and_then(|w| w.to_local(scope))
    {
        parent = owner;
    }
    if !parent
        .get_slot::<FrameLink>()
        .is_some_and(|link| link.active.get())
    {
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
            let Some(child) = create(scope, parent, &node, element, &tree, document, None) else {
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
    } else if child.get_security_token(scope) == caller.get_security_token(scope) {
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
    tree.messages.remove(document);
    tree.ports.remove(document);
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
