//! Coalesced rendering invalidation shared by DOM, style, layout, and diagnostics.

use super::dom::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvalidationImpact(u8);

impl InvalidationImpact {
    pub const STYLE: Self = Self(1 << 0);
    pub const LAYOUT: Self = Self(1 << 1);
    pub const INTRINSIC_SIZE: Self = Self(1 << 2);
    pub const PAINT: Self = Self(1 << 3);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn affects_style(self) -> bool {
        self.0 & Self::STYLE.0 != 0
    }

    pub const fn affects_layout(self) -> bool {
        self.0 & (Self::LAYOUT.0 | Self::INTRINSIC_SIZE.0) != 0
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
    Stylesheet,
    Viewport,
}

impl MutationKind<'_> {
    pub fn impact(self) -> InvalidationImpact {
        match self {
            Self::Attribute(name) => {
                let base = InvalidationImpact::STYLE
                    .union(InvalidationImpact::LAYOUT)
                    .union(InvalidationImpact::PAINT);
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "src" | "srcset" | "sizes" | "width" | "height" | "value"
                ) {
                    base.union(InvalidationImpact::INTRINSIC_SIZE)
                } else {
                    base
                }
            }
            Self::CharacterData => InvalidationImpact::LAYOUT
                .union(InvalidationImpact::INTRINSIC_SIZE)
                .union(InvalidationImpact::PAINT),
            Self::ChildList | Self::Stylesheet => InvalidationImpact::STYLE
                .union(InvalidationImpact::LAYOUT)
                .union(InvalidationImpact::INTRINSIC_SIZE)
                .union(InvalidationImpact::PAINT),
            Self::Viewport => InvalidationImpact::STYLE
                .union(InvalidationImpact::LAYOUT)
                .union(InvalidationImpact::PAINT),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderInvalidation {
    pub root: Option<NodeId>,
    pub impact: InvalidationImpact,
    pub mutation_count: usize,
    pub rebuild_style_rules: bool,
    pub removed_nodes: Vec<NodeId>,
}

impl RenderInvalidation {
    pub fn viewport(root: NodeId) -> Self {
        Self {
            root: Some(root),
            impact: MutationKind::Viewport.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            removed_nodes: Vec::new(),
        }
    }

    pub fn full(root: NodeId) -> Self {
        Self {
            root: Some(root),
            impact: MutationKind::Stylesheet.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            removed_nodes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Combines rendering work produced by sequential callbacks before an embedder checkpoint.
    /// Node identities alone cannot recover ancestry, so distinct roots conservatively widen to
    /// the document. The normal single-root path remains incremental.
    pub fn merge_conservatively(&mut self, mut other: Self, document_root: NodeId) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other;
            return;
        }
        if self.root != other.root {
            self.root = Some(document_root);
        }
        self.impact = self.impact.union(other.impact);
        self.mutation_count = self.mutation_count.saturating_add(other.mutation_count);
        self.rebuild_style_rules |= other.rebuild_style_rules;
        self.removed_nodes.append(&mut other.removed_nodes);
        self.removed_nodes.sort_unstable();
        self.removed_nodes.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_mutations_conservatively() {
        assert!(MutationKind::Attribute("class").impact().affects_style());
        assert!(MutationKind::Attribute("width").impact().affects_layout());
        assert!(MutationKind::CharacterData.impact().affects_layout());
        assert!(!MutationKind::CharacterData.impact().affects_style());
        assert!(MutationKind::ChildList.impact().affects_paint());
    }

    #[test]
    fn merging_distinct_roots_widens_to_the_document() {
        let document = NodeId::from_wire((1_u128 << 64) | 1).unwrap();
        let left = NodeId::from_wire((1_u128 << 64) | 2).unwrap();
        let right = NodeId::from_wire((1_u128 << 64) | 3).unwrap();
        let mut invalidation = RenderInvalidation {
            root: Some(left),
            impact: InvalidationImpact::STYLE,
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: vec![left],
        };
        invalidation.merge_conservatively(
            RenderInvalidation {
                root: Some(right),
                impact: InvalidationImpact::PAINT,
                mutation_count: 2,
                rebuild_style_rules: false,
                removed_nodes: vec![left, right],
            },
            document,
        );

        assert_eq!(invalidation.root, Some(document));
        assert!(invalidation.impact.affects_style());
        assert!(invalidation.impact.affects_paint());
        assert_eq!(invalidation.mutation_count, 3);
        assert_eq!(invalidation.removed_nodes, vec![left, right]);
    }
}
