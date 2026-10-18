//! Conservative cascade invalidation for dependencies outside a dirty subtree.
use super::*;

#[derive(Debug, Default)]
pub(in crate::engine::css::cascade) struct NonlocalDependencies {
    any: bool,
    style_attribute: bool,
    text_sensitive: bool,
    graphics_attribute: bool,
    resource_attribute: bool,
    any_style_attribute: bool,
    /// `:has-slotted` or `:dir()` dependencies, which have no bounded root walk.
    unbounded: bool,
    has_anchors: super::has_anchors::HasAnchors,
    appended_child: AppendedChildDependencies,
}

/// Selectors through which appending a child can restyle elements outside the new subtree.
/// Positions counted from the start are unchanged by an append; these count from the end.
/// https://drafts.csswg.org/selectors-4/#structural-pseudos
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppendedChildDependencies {
    /// `:last-child` or `:only-child`: the previous last element child.
    pub(crate) last_child: bool,
    /// `:last-of-type` or `:only-of-type`: the previous last element of the same type.
    pub(crate) last_of_type: bool,
    /// `:nth-last-child()` or `:nth-last-of-type()`: every earlier sibling.
    pub(crate) nth_last: bool,
    /// `:empty`: the parent, while it had no element or non-whitespace text children.
    pub(crate) empty: bool,
}

impl NonlocalDependencies {
    pub(super) fn for_rules(rules: &[Rule]) -> Self {
        let mut dependencies = Self::default();
        for rule in rules {
            dependencies.include(&rule.selector);
            dependencies.has_anchors.include(&rule.selector);
        }
        dependencies
    }

    /// Replaces a whole-document refresh with bounded roots when `:has()` is the only
    /// nonlocal dependency; `None` keeps the whole-document refresh.
    pub(in crate::engine::css::cascade) fn relational_roots(
        &self,
        requested: &[NodeRef],
        known: &super::has_anchors::HasResults,
    ) -> Option<super::has_anchors::RelationalRoots> {
        if self.unbounded {
            return None;
        }
        self.has_anchors.roots(requested, known)
    }

    pub(in crate::engine::css::cascade) fn appended_child(&self) -> AppendedChildDependencies {
        self.appended_child
    }

    /// See [`super::has_anchors::HasAnchors::current_results`].
    pub(in crate::engine::css::cascade) fn current_has_results(
        &self,
        node: &NodeRef,
    ) -> Option<(Vec<bool>, bool)> {
        if self.unbounded {
            return None;
        }
        self.has_anchors.current_results(node)
    }

    pub(in crate::engine::css::cascade) fn needs_document_refresh(
        &self,
        impact: crate::engine::invalidation::InvalidationImpact,
    ) -> bool {
        // Evidence describes real mutation categories, never a site or selector
        // result guess. Unknown attributes, element membership and state changes
        // retain whole-document refresh; local writes widen for relevant selectors.
        self.any
            && if impact.is_style_only() {
                self.style_attribute
            } else if impact.is_text_only() {
                self.text_sensitive
            } else if impact.is_local_content_only() {
                self.text_sensitive || self.style_attribute || self.graphics_attribute
            } else if impact.is_resource_attribute_only() {
                self.resource_attribute
            } else {
                true
            }
    }

    fn include(&mut self, selector: &Selector) {
        let mut pending = vec![(selector, false)];
        let mut visited = HashSet::new();
        while let Some((selector, relative)) = pending.pop() {
            // Shared functional-selector graphs need at most two visits: inside
            // and outside a relative selector. Do not lose the attribute edge
            // when the same immutable graph appears in both positions.
            if !visited.insert((Rc::as_ptr(&selector.compounds), relative)) {
                continue;
            }
            for compound in selector.compounds.iter() {
                let appended = &mut self.appended_child;
                appended.last_child |= compound.requires_last_child || compound.requires_only_child;
                appended.last_of_type |=
                    compound.requires_last_of_type || compound.requires_only_of_type;
                appended.nth_last |= compound.nth.iter().any(|nth| nth.from_end);
                appended.empty |= compound.requires_empty;
                self.any_style_attribute |= compound
                    .attributes
                    .iter()
                    .any(|attribute| attribute.name.eq_ignore_ascii_case("style"));
                // Text-only child replacement cannot change element membership,
                // but can change :empty, auto directionality and control validity.
                // Nested :is/:not/nth filters retain the relative dependency context.
                self.text_sensitive |= compound.requires_has_slotted
                    || !compound.directions.is_empty()
                    || relative
                        && (compound.requires_empty
                            || !compound.directions.is_empty()
                            || compound.requires_valid
                            || compound.requires_invalid
                            || compound.requires_in_range
                            || compound.requires_out_of_range);
                self.style_attribute |= relative
                    && compound
                        .attributes
                        .iter()
                        .any(|attribute| attribute.name.eq_ignore_ascii_case("style"));
                self.graphics_attribute |= relative
                    && compound.attributes.iter().any(|attribute| {
                        crate::engine::invalidation::is_local_graphics_attribute(&attribute.name)
                    });
                self.resource_attribute |= !compound.directions.is_empty()
                    || compound.requires_has_slotted
                    || relative
                        && compound.attributes.iter().any(|attribute| {
                            crate::engine::invalidation::is_resource_attribute(&attribute.name)
                        });
                // Selectors 4 §4.5 allows descendant and later-sibling dependencies.
                // https://drafts.csswg.org/selectors/#relational
                self.any |= !compound.has.is_empty()
                    || compound.requires_has_slotted
                    || !compound.directions.is_empty();
                self.unbounded |= compound.requires_has_slotted || !compound.directions.is_empty();
                // Slot assignment has separate cross-tree rules. Do not infer a
                // local refresh from the absence of an ordinary [style] selector.
                self.style_attribute |= compound.requires_has_slotted;
                for relatives in &compound.has {
                    pending.extend(relatives.iter().map(|relative| (&relative.selector, true)));
                }
                for functional in &compound.functional {
                    pending.extend(
                        functional
                            .selectors
                            .iter()
                            .map(|selector| (selector, relative)),
                    );
                }
                for nth in &compound.nth {
                    pending.extend(nth.filter.iter().map(|selector| (selector, relative)));
                }
            }
        }
    }
}

pub(super) fn cross_tree_or_scoped(rules: &[Rule]) -> bool {
    rules.iter().any(|rule| {
        rule.scope != RuleScope::Document
            || !rule.css_scopes.is_empty()
            || rule.part.is_some()
            || rule.host_condition.is_some()
            || rule.slotted_origin.is_some()
    })
}

impl StyleSet {
    pub(crate) fn appended_child_dependencies(&self) -> AppendedChildDependencies {
        self.compiled.nonlocal_dependencies.appended_child()
    }

    pub(crate) fn can_scope_fixed_content(
        &self,
        impact: crate::engine::invalidation::InvalidationImpact,
    ) -> bool {
        !self.compiled.cross_tree_or_scoped
            && impact.is_local_content_only()
            && !self
                .compiled
                .nonlocal_dependencies
                .needs_document_refresh(impact)
    }

    pub(crate) fn can_scope_inline_style_to_target(&self) -> bool {
        // A declaration can affect inherited descendants and geometry, but
        // only attribute selectors can observe the authored style text. Scope,
        // host, part and slot graphs retain their existing conservative roots.
        !self.compiled.cross_tree_or_scoped
            && !self.compiled.nonlocal_dependencies.any_style_attribute
    }

    pub(crate) fn can_prove_unchanged_presentation_styles(&self) -> bool {
        // Shadow parts and scoped cascades keep their existing refresh policy.
        // Do not widen every shadow edit into an eager document recomputation,
        // or discard deferred display:none subtrees merely to enable a shortcut.
        !self.compiled.cross_tree_or_scoped
    }
}

#[cfg(test)]
mod style_scope_tests;
#[cfg(test)]
mod tests;
