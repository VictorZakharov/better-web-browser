//! Conservative cascade invalidation for dependencies outside a dirty subtree.
use super::*;

#[derive(Debug, Default)]
pub(in crate::engine::css::cascade) struct NonlocalDependencies {
    any: bool,
    style_attribute: bool,
    text_sensitive: bool,
    graphics_attribute: bool,
    any_style_attribute: bool,
}

impl NonlocalDependencies {
    pub(super) fn for_rules(rules: &[Rule]) -> Self {
        let mut dependencies = Self::default();
        for rule in rules {
            dependencies.include(&rule.selector);
        }
        dependencies
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
                // Selectors 4 §4.5 allows descendant and later-sibling dependencies.
                // https://drafts.csswg.org/selectors/#relational
                self.any |= !compound.has.is_empty()
                    || compound.requires_has_slotted
                    || !compound.directions.is_empty();
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
