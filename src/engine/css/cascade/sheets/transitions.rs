use super::*;

pub(super) fn rule_indices(rules: &[Rule]) -> Vec<usize> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            rule.declarations.iter().any(|declaration| {
                declaration.name == "all"
                    || super::super::super::values::transitions::supported_property(
                        &declaration.name,
                    )
            })
        })
        .map(|(index, _)| index)
        .collect()
}

pub(super) fn potential_rule_indices(rules: &[Rule], indices: &[usize]) -> Vec<usize> {
    indices
        .iter()
        .copied()
        .filter(|&index| {
            rules[index]
                .declarations
                .iter()
                .any(declaration_may_set_transition_time)
        })
        .collect()
}

fn declaration_may_set_transition_time(declaration: &Declaration) -> bool {
    if declaration.name == "all" {
        return declaration.may_use_var || declaration.value.trim().eq_ignore_ascii_case("inherit");
    }
    if !matches!(
        declaration.name.as_str(),
        "transition" | "transition-duration" | "transition-delay"
    ) {
        return false;
    }
    if declaration.may_use_var {
        return true;
    }
    let mut settings = super::super::super::values::transitions::TransitionSettings::default();
    if !super::super::super::values::transitions::apply(
        &mut settings,
        &declaration.name,
        &declaration.value,
    ) {
        // CSS-wide values, including inherit, are handled by the cascade.
        return true;
    }
    settings.durations.iter().any(|duration| *duration > 0.0)
        || settings.delays.iter().any(|delay| *delay > 0.0)
        || settings
            .calculated_times
            .durations
            .iter()
            .any(Option::is_some)
        || settings.calculated_times.delays.iter().any(Option::is_some)
}

impl StyleSet {
    pub(crate) fn attribute_change_can_affect_style(
        &self,
        node: &NodeRef,
        name: &str,
        next: &str,
    ) -> bool {
        if name == "style" {
            return true;
        }
        // Every style rule is relevant here, not only transition rules: an
        // ancestor selector can change an inherited value on a descendant
        // whose transition is declared separately.
        self.compiled.attribute_dependencies.may_change(
            name,
            &node.attr(name).unwrap_or_default(),
            next,
        )
    }

    pub(crate) fn transition_settings_may_change_on_attribute(
        &self,
        node: &NodeRef,
        name: &str,
        next: &str,
    ) -> bool {
        self.compiled.transition_setting_dependencies.may_change(
            name,
            &node.attr(name).unwrap_or_default(),
            next,
        )
    }

    pub(crate) fn has_transition_rules(&self) -> bool {
        !self.compiled.potential_transition_rule_indices.is_empty()
    }
    /// Conservative rightmost-selector filter for an element's own attribute
    /// change. Unknown functional/attribute conditions stay candidates.
    pub(crate) fn may_transition_on_attribute(
        &self,
        node: &NodeRef,
        name: &str,
        next_value: &str,
    ) -> bool {
        self.may_match_transition_rules(
            node,
            name,
            next_value,
            self.compiled.transition_rule_indices.iter().copied(),
        )
    }

    /// [`Self::may_transition_on_attribute`] for an element whose own attributes are not
    /// changing, such as a descendant of the mutated element. Its candidate rules come from
    /// the transition rule index instead of a scan of every transition rule.
    pub(crate) fn may_transition_unchanged(&self, node: &NodeRef) -> bool {
        self.may_match_transition_rules(
            node,
            "",
            "",
            self.compiled.transition_index.candidates(node, None),
        )
    }

    pub(crate) fn may_have_transition_time_rule(&self, node: &NodeRef) -> bool {
        self.compiled
            .potential_transition_index
            .candidates(node, None)
            .into_iter()
            .any(|index| {
                let rule = &self.compiled.rules[index];
                // Shadow, part, slot and scope matching have additional roots.
                // Keep them conservative; the ordinary selector matcher is a
                // sufficient fast rejection for document-scoped rules.
                rule.scope != RuleScope::Document
                    || !rule.css_scopes.is_empty()
                    || rule.host_condition.is_some()
                    || rule.part.is_some()
                    || rule.slotted_origin.is_some()
                    || crate::engine::css::selector_match::selector_matches(&rule.selector, node)
            })
    }

    fn may_match_transition_rules(
        &self,
        node: &NodeRef,
        name: &str,
        next_value: &str,
        indices: impl IntoIterator<Item = usize>,
    ) -> bool {
        let old_id = node.attr("id").unwrap_or_default();
        let old_class = node.attr("class").unwrap_or_default();
        let next_id = if name == "id" { next_value } else { &old_id };
        let next_class = if name == "class" {
            next_value
        } else {
            &old_class
        };
        indices.into_iter().any(|index| {
            let rule = &self.compiled.rules[index];
            if rule.pseudo.is_some() {
                return false;
            }
            let Some(compound) = rule.selector.compounds.last() else {
                return true;
            };
            if compound
                .tag
                .as_deref()
                .is_some_and(|tag| node.tag_name() != Some(tag))
            {
                return false;
            }
            if compound
                .id
                .as_deref()
                .is_some_and(|id| id != old_id && id != next_id)
            {
                return false;
            }
            compound.classes.iter().all(|class| {
                old_class.split_ascii_whitespace().any(|old| old == class)
                    || next_class.split_ascii_whitespace().any(|new| new == class)
            })
        })
    }
}
