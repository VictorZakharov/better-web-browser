//! Diagnostic counts of style invalidations that could not be bounded by invalidation sets.

/// Why a refresh restyled whole subtrees instead of the elements a change can affect. Each
/// field counts invalidation roots; a checkpoint that also changes stylesheets resolves its
/// attribute changes against both the outgoing and the incoming rules, and counts both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvalidationFallbacks {
    /// A class or attribute change used by a rule with a sibling combinator after it: the
    /// change's parent subtree.
    pub sibling_combinator: u64,
    /// A class or attribute change used inside `:has()`, `:nth-child(of S)` or a complex
    /// `:is()`/`:where()`/`:not()` argument: the change's parent subtree.
    pub nested_selector: u64,
    /// A class or attribute change under rules that cannot be bounded at all (scoped or
    /// cross-tree rules, complex nested `[class]` selectors): the change's parent subtree.
    pub unbounded_rules: u64,
    /// A class or attribute change used in an ancestor compound of a rule whose subject has no
    /// key (`.a *`): the element's whole subtree.
    pub keyless_descendant: u64,
    /// Subtrees a changed stylesheet restyles because its new or removed rules cannot be
    /// narrowed to their subjects.
    pub rule_diff_subtrees: u64,
}

impl InvalidationFallbacks {
    pub const FIELDS: usize = 5;

    pub fn saturating_add(self, other: Self) -> Self {
        let mut sum = self.to_fields();
        for (total, value) in sum.iter_mut().zip(other.to_fields()) {
            *total = total.saturating_add(value);
        }
        Self::from_fields(sum)
    }

    pub fn to_fields(self) -> [u64; Self::FIELDS] {
        [
            self.sibling_combinator,
            self.nested_selector,
            self.unbounded_rules,
            self.keyless_descendant,
            self.rule_diff_subtrees,
        ]
    }

    pub fn from_fields(fields: [u64; Self::FIELDS]) -> Self {
        let [
            sibling_combinator,
            nested_selector,
            unbounded_rules,
            keyless_descendant,
            rule_diff_subtrees,
        ] = fields;
        Self {
            sibling_combinator,
            nested_selector,
            unbounded_rules,
            keyless_descendant,
            rule_diff_subtrees,
        }
    }
}
