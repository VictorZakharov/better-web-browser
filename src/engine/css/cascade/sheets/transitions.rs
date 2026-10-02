use super::*;

pub(super) fn rule_indices(rules: &[Rule]) -> Vec<usize> {
    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| {
            rule.declarations.iter().any(|declaration| {
                super::super::super::values::transitions::supported_property(&declaration.name)
            })
        })
        .map(|(index, _)| index)
        .collect()
}

impl StyleSet {
    pub(crate) fn has_transition_rules(&self) -> bool {
        !self.compiled.transition_rule_indices.is_empty()
    }
    /// Conservative rightmost-selector filter for an element's own attribute
    /// change. Unknown functional/attribute conditions stay candidates.
    pub(crate) fn may_transition_on_attribute(
        &self,
        node: &NodeRef,
        name: &str,
        next_value: &str,
    ) -> bool {
        let old_id = node.attr("id").unwrap_or_default();
        let old_class = node.attr("class").unwrap_or_default();
        let next_id = if name == "id" { next_value } else { &old_id };
        let next_class = if name == "class" {
            next_value
        } else {
            &old_class
        };
        self.compiled.transition_rule_indices.iter().any(|&index| {
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
