//! Registers parser-created nodes with the realm and selects what its parser hook must see.
//
// After every parser step the realm gives new nodes identities and owner documents, wraps the
// elements that need eager work and adds the window names they bring. A navigation parser
// journals the roots it inserted (`Dom::take_parser_registration_roots`), so a step registers
// only those subtrees; other parsers fall back to walking the whole target document.
use super::*;
use std::collections::HashSet;

#[cfg(test)]
mod tests;

/// Elements the parser created since the last registration, for the realm's parser hook.
pub(in crate::engine::script) struct ParserChanges {
    /// Comma-separated IDs, in tree order, of the new elements whose wrappers must exist now:
    /// elements with event handler content attributes, iframes, shadow hosts and possible
    /// custom elements. Other wrappers are created on first use, as for any node.
    pub(in crate::engine::script) ids: String,
    /// Window named-property names (`named_values`) of the new elements in the realm's document
    /// and of its <html> and <body>, whose attributes the parser can merge.
    pub(in crate::engine::script) names: JsValue,
}

impl HostState {
    /// Registers what the navigation parser changed: `roots` from its journal, or the whole
    /// document when there is none.
    pub(in crate::engine::script) fn register_parser_changes(
        &mut self,
        roots: Option<&[NodeRef]>,
    ) -> ParserChanges {
        let document = self.document.clone();
        match roots {
            Some(roots) => self.register_parser_roots(&document, roots),
            None => self.register_parser_changes_in(&document),
        }
    }

    pub(in crate::engine::script) fn register_parser_changes_in(
        &mut self,
        document: &NodeRef,
    ) -> ParserChanges {
        self.invalidate_parser_layout(document);
        let new_elements = Node::shadow_including_descendants(document)
            .filter(|node| self.is_new_parser_element(node))
            .collect::<Vec<_>>();
        self.register_subtree(document);
        self.parser_changes(document, new_elements)
    }

    /// Registers the subtrees of `roots`, which the parser inserted (or whose shadow root it
    /// attached) since the last registration. A root's subtree is complete by now, so a root
    /// already visited inside an earlier one is skipped.
    fn register_parser_roots(&mut self, document: &NodeRef, roots: &[NodeRef]) -> ParserChanges {
        self.invalidate_parser_layout_for(document, roots);
        let mut visited = HashSet::new();
        let mut new_elements = Vec::new();
        for root in roots {
            if visited.contains(&root.id())
                || Node::shadow_including_root(root).id() != document.id()
            {
                continue;
            }
            for node in Node::shadow_including_descendants(root) {
                if visited.insert(node.id()) && self.is_new_parser_element(&node) {
                    new_elements.push(node);
                }
            }
            self.register_subtree(root);
        }
        self.parser_changes(document, new_elements)
    }

    fn invalidate_parser_layout(&mut self, document: &NodeRef) {
        self.clear_parser_layout_caches();
        self.pending_layout_invalidation
            .record(document, Some(document), MutationKind::Stylesheet);
    }

    /// Scopes the layout flush's restyle to the journaled roots, called before they are
    /// registered. An inserted subtree is a child-list change of its parent, which also
    /// rebuilds rules when the subtree holds a style sheet. An element the realm already knew
    /// had its attributes or shadow root changed, which may add style sheets too, so its
    /// parent's subtree restyles against rebuilt rules.
    fn invalidate_parser_layout_for(&mut self, document: &NodeRef, roots: &[NodeRef]) {
        self.clear_parser_layout_caches();
        for root in roots {
            if Node::shadow_including_root(root).id() != document.id() {
                continue;
            }
            let Some(parent) = root.shadow_including_parent() else {
                continue;
            };
            let kind = if self.node_ids.contains_key(&root.id())
                || crate::engine::script::mutation_host::subtree_contains_style(root)
            {
                MutationKind::Stylesheet
            } else {
                MutationKind::ChildList
            };
            self.pending_layout_invalidation
                .record(document, Some(&parent), kind);
        }
    }

    fn clear_parser_layout_caches(&mut self) {
        self.computed_styles = None;
        self.offset_parent_styles = None;
        self.layout_geometry_initialized = false;
    }

    fn is_new_parser_element(&self, node: &NodeRef) -> bool {
        node.element().is_some()
            && (!self.node_ids.contains_key(&node.id())
                || node
                    .shadow_root()
                    .is_some_and(|root| !self.node_ids.contains_key(&root.id())))
    }

    fn parser_changes(&mut self, document: &NodeRef, new_elements: Vec<NodeRef>) -> ParserChanges {
        let mut names = Vec::new();
        if document.id() == self.document.id() {
            let child_element = |parent: &NodeRef, tag: Option<&str>| {
                parent
                    .children
                    .borrow()
                    .iter()
                    .find(|child| {
                        child.element().is_some()
                            && tag.is_none_or(|tag| child.tag_name() == Some(tag))
                    })
                    .cloned()
            };
            let root = child_element(&self.document, None);
            let body = root
                .as_ref()
                .and_then(|root| child_element(root, Some("body")));
            for node in new_elements.iter().chain(root.iter()).chain(body.iter()) {
                if Node::tree_root(node).id() == self.document.id() {
                    names.extend(
                        super::super::dom_host::named_values(node)
                            .into_iter()
                            .map(JsValue::from),
                    );
                }
            }
        }
        let mut eager = new_elements
            .into_iter()
            .filter(needs_parser_wrapper)
            .collect::<Vec<_>>();
        // Journal roots follow insertion order, which an insertion before a sibling (foster
        // parenting, for example) takes out of tree order. Custom-element upgrades observe it.
        if eager.len() > 1 {
            eager.sort_by_cached_key(tree_path);
        }
        let ids = eager
            .iter()
            .map(|node| self.id_for(node).to_string())
            .collect::<Vec<_>>()
            .join(",");
        ParserChanges {
            ids,
            names: JsValue::Array(names),
        }
    }
}

/// Whether the parser hook must wrap `element` as soon as it is parsed: wrapping activates
/// event handler content attributes, and the hook upgrades custom elements (whose names
/// contain a hyphen), exposes iframe and object windows and registers shadow roots for traversal.
fn needs_parser_wrapper(element: &NodeRef) -> bool {
    element.shadow_root().is_some()
        || element
            .tag_name()
            .is_some_and(|name| matches!(name, "iframe" | "object") || name.contains('-'))
        || element.element().is_some_and(|data| {
            data.attrs.borrow().iter().any(|attribute| {
                attribute.name.ns.as_ref().is_empty()
                    && attribute.name.local.as_ref().starts_with("on")
            })
        })
}

/// A key that orders nodes as `Node::shadow_including_descendants` visits them: a node, then
/// its shadow root's subtree, then its children.
fn tree_path(node: &NodeRef) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = node.clone();
    while let Some(parent) = current.shadow_including_parent() {
        let index = if parent
            .shadow_root()
            .is_some_and(|root| root.id() == current.id())
        {
            0
        } else {
            1 + parent
                .children
                .borrow()
                .iter()
                .position(|child| child.id() == current.id())
                .unwrap_or_default()
        };
        path.push(index);
        current = parent;
    }
    path.reverse();
    path
}
