//! Coalesced rendering invalidation shared by DOM, style, layout, and diagnostics.

use super::dom::NodeId;
use super::dom::{Node, NodeRef};

pub(crate) const MAX_INVALIDATION_ROOTS: usize = 256;
/// Roots kept for checkpoints that skipped their frame while a document streams in. Each
/// parser checkpoint adds up to `MAX_INVALIDATION_ROOTS` appended subtrees; collapsing their
/// union to the whole document would restyle everything styled by earlier frames again.
pub(crate) const MAX_DEFERRED_INVALIDATION_ROOTS: usize = 16 * MAX_INVALIDATION_ROOTS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvalidationImpact(u8);

// Evidence, not an impact: only inline/animation-style writes may take an exact
// post-cascade equality proof. Mixing any other kind clears that evidence.
const STYLE_ONLY: u8 = 1 << 4;
// Proven replacement of text/comment children only, not element tree membership.
const TEXT_ONLY: u8 = 1 << 5;
// Coalesced declarations, proven text children, and known graphics attributes.
// Arbitrary attributes, element insertion, state and unknown work clear this proof.
const LOCAL_CONTENT_ONLY: u8 = 1 << 6;
// Replaced-resource attributes can change intrinsic geometry, while their
// effect on a relational selector requires that selector to read the attribute.
const RESOURCE_ATTRIBUTE_ONLY: u8 = 1 << 7;
mod attributes;
mod class_changes;
pub(crate) use attributes::is_local_graphics_attribute;
pub use class_changes::ClassChange;
pub(crate) use class_changes::{changed_class_tokens, record_class_change};

impl InvalidationImpact {
    pub const STYLE: Self = Self(1 << 0);
    pub const LAYOUT: Self = Self(1 << 1);
    pub const INTRINSIC_SIZE: Self = Self(1 << 2);
    pub const PAINT: Self = Self(1 << 3);

    pub const fn union(self, other: Self) -> Self {
        let only_style = (self.is_style_only() || other.is_style_only())
            && (self.0 == 0 || self.is_style_only())
            && (other.0 == 0 || other.is_style_only());
        let only_text = (self.is_text_only() || other.is_text_only())
            && (self.0 == 0 || self.is_text_only())
            && (other.0 == 0 || other.is_text_only());
        let only_local = (self.is_local_content_only() || other.is_local_content_only())
            && (self.0 == 0 || self.is_local_content_only())
            && (other.0 == 0 || other.is_local_content_only());
        let only_resource = (self.is_resource_attribute_only()
            || other.is_resource_attribute_only())
            && (self.0 == 0 || self.is_resource_attribute_only())
            && (other.0 == 0 || other.is_resource_attribute_only());
        Self(
            ((self.0 | other.0)
                & !(STYLE_ONLY | TEXT_ONLY | LOCAL_CONTENT_ONLY | RESOURCE_ATTRIBUTE_ONLY))
                | if only_style { STYLE_ONLY } else { 0 }
                | if only_text { TEXT_ONLY } else { 0 }
                | if only_local { LOCAL_CONTENT_ONLY } else { 0 }
                | if only_resource {
                    RESOURCE_ATTRIBUTE_ONLY
                } else {
                    0
                },
        )
    }

    pub(crate) const fn is_style_only(self) -> bool {
        self.0 & STYLE_ONLY != 0
    }

    pub(crate) const fn is_text_only(self) -> bool {
        self.0 & TEXT_ONLY != 0
    }

    pub(crate) const fn is_local_content_only(self) -> bool {
        self.0 & LOCAL_CONTENT_ONLY != 0
    }

    pub(crate) const fn is_resource_attribute_only(self) -> bool {
        self.0 & RESOURCE_ATTRIBUTE_ONLY != 0
    }

    pub const fn affects_style(self) -> bool {
        self.0 & Self::STYLE.0 != 0
    }

    pub const fn affects_layout(self) -> bool {
        self.0 & (Self::LAYOUT.0 | Self::INTRINSIC_SIZE.0) != 0
    }

    pub const fn affects_intrinsic_size(self) -> bool {
        self.0 & Self::INTRINSIC_SIZE.0 != 0
    }

    pub(crate) const fn without_intrinsic_size(self) -> Self {
        Self(self.0 & !Self::INTRINSIC_SIZE.0)
    }

    pub const fn affects_paint(self) -> bool {
        self.0 & Self::PAINT.0 != 0
    }

    pub fn labels(self) -> String {
        let mut labels = Vec::new();
        if self.affects_style() {
            labels.push("style");
        }
        if self.0 & Self::LAYOUT.0 != 0 {
            labels.push("layout");
        }
        if self.0 & Self::INTRINSIC_SIZE.0 != 0 {
            labels.push("intrinsic-size");
        }
        if self.affects_paint() {
            labels.push("paint");
        }
        labels.join("+")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationKind<'a> {
    Attribute(&'a str),
    CharacterData,
    ChildList,
    /// Proven HTML text-only child replacement; new text-node identity still needs style.
    TextChildren,
    Stylesheet,
    Viewport,
    /// User-agent state (for example pointer designation), not an attribute mutation.
    State,
    /// Pointer designation can change selectors, but does not itself change content or controls.
    PointerDesignation,
    /// Native CSS animation/transition declarations, not DOM or control state.
    StyleOverlay,
}

impl MutationKind<'_> {
    pub fn impact(self) -> InvalidationImpact {
        match self {
            Self::PointerDesignation => InvalidationImpact::STYLE,
            Self::StyleOverlay => InvalidationImpact(
                InvalidationImpact::STYLE.0
                    | InvalidationImpact::LAYOUT.0
                    | InvalidationImpact::PAINT.0
                    | STYLE_ONLY
                    | LOCAL_CONTENT_ONLY,
            ),
            Self::Attribute(name) => {
                let base = InvalidationImpact::STYLE
                    .union(InvalidationImpact::LAYOUT)
                    .union(InvalidationImpact::PAINT);
                let base = if is_local_graphics_attribute(name) {
                    InvalidationImpact(base.0 | LOCAL_CONTENT_ONLY)
                } else {
                    base
                };
                if name.eq_ignore_ascii_case("style") {
                    InvalidationImpact(base.0 | STYLE_ONLY | LOCAL_CONTENT_ONLY)
                } else if is_resource_attribute(name) {
                    InvalidationImpact(
                        base.union(InvalidationImpact::INTRINSIC_SIZE).0 | RESOURCE_ATTRIBUTE_ONLY,
                    )
                } else if name.eq_ignore_ascii_case("value") {
                    base.union(InvalidationImpact::INTRINSIC_SIZE)
                } else {
                    base
                }
            }
            Self::CharacterData => InvalidationImpact::LAYOUT
                .union(InvalidationImpact::INTRINSIC_SIZE)
                .union(InvalidationImpact::PAINT),
            Self::TextChildren => InvalidationImpact(
                InvalidationImpact::STYLE.0
                    | InvalidationImpact::LAYOUT.0
                    | InvalidationImpact::INTRINSIC_SIZE.0
                    | InvalidationImpact::PAINT.0
                    | TEXT_ONLY
                    | LOCAL_CONTENT_ONLY,
            ),
            Self::ChildList | Self::Stylesheet => InvalidationImpact::STYLE
                .union(InvalidationImpact::LAYOUT)
                .union(InvalidationImpact::INTRINSIC_SIZE)
                .union(InvalidationImpact::PAINT),
            Self::Viewport | Self::State => InvalidationImpact::STYLE
                .union(InvalidationImpact::LAYOUT)
                .union(InvalidationImpact::PAINT),
        }
    }
}

pub(crate) fn is_resource_attribute(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "src" | "srcset" | "sizes" | "width" | "height"
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderInvalidation {
    pub roots: Vec<NodeId>,
    pub impact: InvalidationImpact,
    pub mutation_count: usize,
    pub rebuild_style_rules: bool,
    pub removed_nodes: Vec<NodeId>,
    /// True only when removed subtrees were inspected for nonlocal rendering dependencies.
    pub removals_are_local: bool,
    /// Class token changes whose restyle scope the cascade bounds. Their elements are also
    /// conservative subtree roots for consumers that only need to know what may have changed.
    pub class_changes: Vec<ClassChange>,
    /// Roots recorded only because their child lists changed. They stay in `roots` for
    /// consumers that need every possibly changed subtree; the cascade may bound their restyle
    /// to new children, the parent itself and siblings that structural selectors can reach.
    pub child_list_roots: Vec<NodeId>,
}

impl RenderInvalidation {
    pub fn viewport(root: NodeId) -> Self {
        Self {
            roots: vec![root],
            impact: MutationKind::Viewport.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            removed_nodes: Vec::new(),
            removals_are_local: false,
            class_changes: Vec::new(),
            child_list_roots: Vec::new(),
        }
    }

    pub fn full(root: NodeId) -> Self {
        Self {
            roots: vec![root],
            impact: MutationKind::Stylesheet.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            removed_nodes: Vec::new(),
            removals_are_local: false,
            class_changes: Vec::new(),
            child_list_roots: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty() && self.class_changes.is_empty()
    }

    /// Every node whose subtree may contain changed rendering: the roots plus elements with
    /// class changes. Sibling-dependent class changes already add their parent to the roots.
    pub fn affected_roots(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.roots
            .iter()
            .copied()
            .chain(self.class_changes.iter().map(|change| change.node))
    }

    /// Combines rendering work produced by sequential callbacks before an embedder checkpoint.
    /// Ancestry is normalized against the live DOM at refresh time. Keep a bounded set here so
    /// independent component updates do not become a document-wide style pass.
    pub fn merge_conservatively(&mut self, other: Self, document_root: NodeId) {
        self.merge_with_root_limit(other, document_root, MAX_INVALIDATION_ROOTS);
    }

    /// `merge_conservatively` with a different bound on the merged roots.
    pub(crate) fn merge_with_root_limit(
        &mut self,
        mut other: Self,
        document_root: NodeId,
        limit: usize,
    ) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other;
            return;
        }
        // A child-list root stays bounded only while no other mutation made it a full root.
        let full = self
            .roots
            .iter()
            .filter(|root| !self.child_list_roots.contains(root))
            .chain(
                other
                    .roots
                    .iter()
                    .filter(|root| !other.child_list_roots.contains(root)),
            )
            .copied()
            .collect::<std::collections::HashSet<_>>();
        self.child_list_roots.append(&mut other.child_list_roots);
        self.child_list_roots.retain(|root| !full.contains(root));
        self.child_list_roots.sort_unstable();
        self.child_list_roots.dedup();
        self.roots.append(&mut other.roots);
        self.roots.sort_unstable();
        self.roots.dedup();
        for change in other.class_changes {
            class_changes::merge_class_change(&mut self.class_changes, change);
        }
        if self.roots.contains(&document_root)
            || self.roots.len() > limit
            || self.class_changes.len() > limit
        {
            self.roots.clear();
            self.roots.push(document_root);
            self.class_changes.clear();
            self.child_list_roots.clear();
        }
        self.impact = self.impact.union(other.impact);
        self.mutation_count = self.mutation_count.saturating_add(other.mutation_count);
        self.rebuild_style_rules |= other.rebuild_style_rules;
        if !other.removed_nodes.is_empty() {
            self.removals_are_local = (self.removed_nodes.is_empty() || self.removals_are_local)
                && other.removals_are_local;
        }
        self.removed_nodes.append(&mut other.removed_nodes);
        self.removed_nodes.sort_unstable();
        self.removed_nodes.dedup();
    }
}

/// Extra style roots whose own selectors can flip when a control's validation
/// state changes: the form owner (HTML `form:valid` / `form:invalid`
/// aggregation, including external `form=` controls) and ancestor fieldsets.
/// The control itself stays covered by the `State`-mutation root (its parent
/// subtree, which also covers sibling-combinator rules); unrelated subtrees
/// are never touched, so a keystroke in one control does not restyle the
/// whole document. Returns an empty set when no form or fieldset is involved.
pub(crate) fn validation_aggregation_roots(control: &NodeRef) -> Vec<NodeRef> {
    let mut roots = Vec::new();
    if matches!(control.tag_name(), Some("form" | "fieldset")) {
        roots.push(control.clone());
    }
    if let Some(owner) = control.form_owner()
        && owner != control.id()
    {
        let root = Node::tree_root(control);
        if let Some(form) = Node::descendants(&root).find(|node| node.id() == owner) {
            roots.push(form);
        }
    }
    let mut ancestor = control.parent();
    while let Some(node) = ancestor {
        if node.tag_name() == Some("fieldset") {
            roots.push(node.clone());
        }
        ancestor = node.parent();
    }
    roots
}

/// Content attributes whose change can flip validation selectors on the
/// element itself (`:required`, `:optional`, `:valid`, `:invalid`,
/// `:in-range`, `:out-of-range`) or on its form/fieldset aggregates.
pub(crate) fn is_validation_attribute(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "value"
            | "checked"
            | "selected"
            | "required"
            | "readonly"
            | "disabled"
            | "type"
            | "min"
            | "max"
            | "step"
            | "pattern"
            | "minlength"
            | "maxlength"
            | "multiple"
            | "form"
            | "name"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    mod local_content;
    mod merge_limits;
    mod removals;
    mod style_only;
    use crate::engine::dom::parse;

    #[test]
    fn validation_roots_cover_owner_and_fieldsets_not_documents() {
        let dom = parse(
            "<form id=f><fieldset><div><input required></div></fieldset></form>\
             <form id=g></form><input required>",
        );
        let input = dom.elements_named("input").next().expect("input");
        let roots: Vec<NodeId> = validation_aggregation_roots(&input)
            .iter()
            .map(|root| root.id())
            .collect();
        let form = dom.elements_named("form").next().expect("form");
        let fieldset = dom.elements_named("fieldset").next().expect("fieldset");
        let other = dom.elements_named("form").nth(1).expect("other form");
        assert!(roots.contains(&form.id()));
        assert!(roots.contains(&fieldset.id()));
        assert!(!roots.contains(&other.id()));
        assert!(!roots.contains(&dom.document.id()));
        // A control without an owner or fieldset needs no extra roots: its
        // own subtree root already covers it, and unrelated nodes stay clean.
        let bare = dom.elements_named("input").nth(1).expect("bare input");
        assert!(validation_aggregation_roots(&bare).is_empty());
    }

    #[test]
    fn classifies_mutations_conservatively() {
        assert!(MutationKind::Attribute("class").impact().affects_style());
        assert!(MutationKind::Attribute("width").impact().affects_layout());
        assert!(
            MutationKind::Attribute("width")
                .impact()
                .affects_intrinsic_size()
        );
        assert!(
            !MutationKind::Attribute("aria-label")
                .impact()
                .affects_intrinsic_size()
        );
        assert!(MutationKind::CharacterData.impact().affects_layout());
        assert!(!MutationKind::CharacterData.impact().affects_style());
        assert!(MutationKind::ChildList.impact().affects_paint());
    }

    #[test]
    fn merging_distinct_roots_preserves_bounded_independent_work() {
        let document = NodeId::from_wire((1_u128 << 64) | 1).unwrap();
        let left = NodeId::from_wire((1_u128 << 64) | 2).unwrap();
        let right = NodeId::from_wire((1_u128 << 64) | 3).unwrap();
        let mut invalidation = RenderInvalidation {
            roots: vec![left],
            impact: InvalidationImpact::STYLE,
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: vec![left],
            removals_are_local: true,
            class_changes: Vec::new(),
            child_list_roots: Vec::new(),
        };
        invalidation.merge_conservatively(
            RenderInvalidation {
                roots: vec![right],
                impact: InvalidationImpact::PAINT,
                mutation_count: 2,
                rebuild_style_rules: false,
                removed_nodes: vec![left, right],
                removals_are_local: false,
                class_changes: Vec::new(),
                child_list_roots: Vec::new(),
            },
            document,
        );

        assert_eq!(invalidation.roots, vec![left, right]);
        assert!(invalidation.impact.affects_style());
        assert!(invalidation.impact.affects_paint());
        assert_eq!(invalidation.mutation_count, 3);
        assert_eq!(invalidation.removed_nodes, vec![left, right]);
        assert!(!invalidation.removals_are_local);
    }
}
