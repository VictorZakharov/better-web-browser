//! Bounded style roots for the edits an HTML parser made since the last rendering checkpoint.
//
// The tree builder inserts a new element before giving it children, and nearly always appends
// at the end of its insertion point. Appending cannot change how earlier siblings match
// selectors that count from the start, so the new subtree is the only style root unless the
// previous rules count from the end (`:last-child`, `:nth-last-of-type()`, ...) or test the
// parent's `:empty`. Those rules styled the earlier siblings, so their dependencies decide which
// neighbors widen the roots. A changed rule set restyles its own subjects in the cascade, and
// relational `:has()` anchors widen inside the style refresh itself.
// https://drafts.csswg.org/selectors-4/#structural-pseudos
use super::*;
use crate::engine::css::cascade::AppendedChildDependencies;
use crate::engine::dom::Node;
use crate::engine::dom::document::parser_render_changes::ParserRenderChange;
use crate::engine::invalidation::{
    MAX_INVALIDATION_ROOTS, MutationKind, RenderInvalidation, validation_aggregation_roots,
};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

#[derive(Default)]
struct ParserRoots {
    roots: Vec<NodeRef>,
    removed: Vec<NodeId>,
    rebuild_style_rules: bool,
    document_wide: bool,
}

impl Page {
    /// Converts parser edits into the rendering invalidation for the next checkpoint.
    /// Returns an empty invalidation when the parser changed nothing.
    pub(crate) fn parser_render_invalidation(
        &self,
        changes: &[ParserRenderChange],
    ) -> RenderInvalidation {
        if changes.is_empty() {
            return RenderInvalidation::default();
        }
        let document = &self.dom.document;
        let styles = self
            .cached_styles
            .as_ref()
            .or(self.previous_styles.as_ref())
            .map(|(_, _, styles)| styles);
        // Without previously computed styles the next checkpoint computes everything anyway.
        let Some(styles) = styles else {
            return RenderInvalidation::full(document.id());
        };
        let dependencies = styles.appended_child_dependencies();
        let mut collected = ParserRoots::default();
        for change in changes {
            collected.include(document, change, dependencies);
        }
        let roots = (!collected.document_wide)
            .then(|| bounded_roots(document, &collected.roots))
            .flatten();
        let Some(roots) = roots else {
            return RenderInvalidation::full(document.id());
        };
        RenderInvalidation {
            roots,
            impact: MutationKind::ChildList.impact(),
            mutation_count: changes.len(),
            rebuild_style_rules: collected.rebuild_style_rules,
            removed_nodes: collected.removed,
            removals_are_local: false,
            class_changes: Vec::new(),
        }
    }
}

impl ParserRoots {
    fn include(
        &mut self,
        document: &NodeRef,
        change: &ParserRenderChange,
        dependencies: AppendedChildDependencies,
    ) {
        match change {
            ParserRenderChange::Inserted {
                parent,
                child,
                appended,
                parent_was_empty,
            } => {
                self.rebuild_style_rules |= affects_style_rules(parent, child);
                if parent.id() == document.id() || document_wide(child) {
                    self.document_wide = true;
                    return;
                }
                self.roots.push(child.clone());
                self.include_control_dependents(parent, child);
                if !appended || dependencies.nth_last || (dependencies.empty && *parent_was_empty) {
                    self.roots.push(parent.clone());
                    return;
                }
                if child.element().is_none() || child.parent().is_none_or(|p| p.id() != parent.id())
                {
                    return;
                }
                if dependencies.last_child
                    && let Some(previous) = previous_element(child, |_| true)
                {
                    self.roots.push(previous);
                }
                if dependencies.last_of_type
                    && let Some(previous) = previous_element(child, |node| same_type(node, child))
                {
                    self.roots.push(previous);
                }
            }
            ParserRenderChange::Removed { parent, child } => {
                self.rebuild_style_rules |= affects_style_rules(parent, child);
                self.document_wide |= document_wide(child);
                self.roots.push(parent.clone());
                self.removed
                    .extend(Node::shadow_including_descendants(child).map(|node| node.id()));
            }
            ParserRenderChange::Text {
                node,
                parent_was_empty,
            } => {
                let parent = node.parent();
                self.rebuild_style_rules |= parent
                    .as_ref()
                    .is_some_and(|parent| parent.tag_name() == Some("style"));
                match parent {
                    Some(parent) if dependencies.empty && *parent_was_empty => {
                        self.roots.push(parent)
                    }
                    Some(parent) if parent.tag_name() == Some("textarea") => {
                        self.include_control_dependents(&parent, node);
                        self.roots.push(parent);
                    }
                    _ => self.roots.push(node.clone()),
                }
            }
            ParserRenderChange::Element(node) => {
                self.rebuild_style_rules |= matches!(node.tag_name(), Some("style" | "link"));
                self.document_wide |= node.tag_name() == Some("html");
                // Attribute selectors can match through sibling combinators.
                self.roots.push(
                    node.shadow_including_parent()
                        .unwrap_or_else(|| node.clone()),
                );
            }
        }
    }

    /// Form-control state that a new node can change outside its own subtree.
    fn include_control_dependents(&mut self, parent: &NodeRef, child: &NodeRef) {
        // Options and default textarea values renormalize their control's state.
        let control = std::iter::successors(Some(parent.clone()), |node| node.parent())
            .take_while(|node| !matches!(node.tag_name(), Some("form" | "html" | "body")))
            .find(|node| matches!(node.tag_name(), Some("select" | "textarea")));
        if let Some(control) = control {
            self.roots.extend(validation_aggregation_roots(&control));
            self.roots.push(control);
        }
        if child.element().is_none() {
            return;
        }
        if child.tag_name() == Some("legend") {
            // A fieldset's first legend exempts its descendants from `:disabled`.
            self.roots.push(parent.clone());
        }
        let owners = validation_aggregation_roots(child);
        // A checked radio unchecks the rest of its group, which spans its form or document.
        let radio = child.tag_name() == Some("input")
            && child
                .attr("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"));
        if radio && !owners.iter().any(|owner| owner.tag_name() == Some("form")) {
            self.document_wide = true;
        }
        self.roots.extend(owners);
    }
}

/// Insertions whose effect is not confined to the inserted subtree's selectors: the document
/// base URL, document metadata (`color-scheme`, `Content-Language`, CSP) and the root element.
fn document_wide(node: &NodeRef) -> bool {
    matches!(node.tag_name(), Some("base" | "meta" | "html"))
        && node.namespace_uri() == Some("http://www.w3.org/1999/xhtml")
}

/// The parser creates elements before their children, so an inserted subtree with children is
/// one it moved (adoption agency, foster parenting); its stylesheets may change rule order.
fn affects_style_rules(parent: &NodeRef, child: &NodeRef) -> bool {
    let stylesheet = |node: &NodeRef| matches!(node.tag_name(), Some("style" | "link"));
    parent.tag_name() == Some("style")
        || stylesheet(child)
        || (!child.children.borrow().is_empty() && Node::descendants(child).any(|n| stylesheet(&n)))
}

fn same_type(left: &NodeRef, right: &NodeRef) -> bool {
    left.tag_name() == right.tag_name() && left.namespace_uri() == right.namespace_uri()
}

fn previous_element(node: &NodeRef, matches: impl Fn(&NodeRef) -> bool) -> Option<NodeRef> {
    let parent = node.parent()?;
    let children = parent.children.borrow();
    let index = children.iter().position(|child| child.id() == node.id())?;
    children[..index]
        .iter()
        .rev()
        .find(|child| child.element().is_some() && matches(child))
        .cloned()
}

/// Connected, disjoint roots in first-seen order, or `None` when the set is too large to be
/// cheaper than a whole-document refresh. Shadow-tree content restyles from its host, since
/// slot assignment and `:host` rules couple the two trees.
fn bounded_roots(document: &NodeRef, requested: &[NodeRef]) -> Option<Vec<NodeId>> {
    let mut candidates = Vec::with_capacity(requested.len());
    for node in requested {
        let Some(root) = light_tree_root(document, node) else {
            continue;
        };
        if root.id() == document.id() {
            return None;
        }
        candidates.push(root);
    }
    let ids = candidates
        .iter()
        .map(|node| node.id())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let roots = candidates
        .iter()
        .filter(|node| {
            std::iter::successors(node.shadow_including_parent(), |node| {
                node.shadow_including_parent()
            })
            .all(|ancestor| !ids.contains(&ancestor.id()))
        })
        .filter(|node| seen.insert(node.id()))
        .map(|node| node.id())
        .collect::<Vec<_>>();
    (roots.len() <= MAX_INVALIDATION_ROOTS).then_some(roots)
}

/// Maps `node` to the document-tree node whose subtree covers it, or `None` when it is not
/// connected (inert template contents or nodes the parser moved out again).
fn light_tree_root(document: &NodeRef, node: &NodeRef) -> Option<NodeRef> {
    let mut root = node.clone();
    let mut current = node.clone();
    loop {
        current = match current.shadow_host() {
            Some(host) => {
                root = host.clone();
                host
            }
            None => match current.parent() {
                Some(parent) => parent,
                None => break,
            },
        };
    }
    if current.id() != document.id() {
        return None;
    }
    // A host's light child is slotted: its composed parent is not its tree parent.
    match root.parent() {
        Some(parent) if parent.shadow_root().is_some() => Some(parent),
        _ => Some(root),
    }
}
