//! Native frame relationships, independent of author-overwritable Window properties.
use super::*;

pub(in crate::engine::script::engine) fn child_windows<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context: v8::Local<'s, v8::Context>,
) -> Vec<v8::Local<'s, v8::Object>> {
    let Some(tree) = tree(context) else {
        return Vec::new();
    };
    let Some(host) = super::super::node_wrappers::host(context) else {
        return Vec::new();
    };
    let state = host.borrow();
    let children = tree.children.borrow();
    let mut windows = Vec::new();
    // Tree order, not context allocation order, defines indexed access.
    for node in crate::engine::dom::Node::descendants(&state.document) {
        if let Some(child) = children
            .get(&node.id())
            .filter(|child| child.parent_document == state.document.id())
        {
            let child = v8::Local::new(scope, &child.context);
            windows.push(child.global(scope));
        }
    }
    windows
}

impl FrameTree {
    pub(in crate::engine::script::engine) fn ancestor_origins(
        &self,
        element: NodeId,
    ) -> Vec<crate::fetch::Origin> {
        let children = self.children.borrow();
        let mut current = children.get(&element);
        let mut origins = Vec::new();
        while let Some(child) = current {
            let Some(parent) = child.parent_host.upgrade() else {
                break;
            };
            origins.push(parent.borrow().document_origin.clone());
            current = children
                .values()
                .find(|ancestor| ancestor.host.borrow().document.id() == child.parent_document);
        }
        origins
    }
    pub(in crate::engine::script::engine) fn has_parent(&self, document: NodeId) -> bool {
        self.children
            .borrow()
            .values()
            .any(|child| child.host.borrow().document.id() == document)
    }

    pub(in crate::engine::script::engine) fn is_ancestor(
        &self,
        ancestor: NodeId,
        mut document: NodeId,
    ) -> bool {
        let children = self.children.borrow();
        for _ in 0..256 {
            let Some(child) = children
                .values()
                .find(|child| child.host.borrow().document.id() == document)
            else {
                return false;
            };
            document = child.parent_document;
            if document == ancestor {
                return true;
            }
        }
        false
    }
}
