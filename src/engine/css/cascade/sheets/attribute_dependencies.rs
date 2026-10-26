//! Selector dependencies that can make a class or ID write change presentation.
use super::*;
use crate::engine::css::selector_model::CompoundSelector;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub(super) struct AttributeDependencies {
    classes: HashSet<String>,
    ids: HashSet<String>,
    class_attribute: bool,
    id_attribute: bool,
    style_attribute: bool,
}

/// Selector graphs already included while building, shared by every rule: including a graph
/// twice adds nothing, and nested arguments are often shared between rules.
#[derive(Default)]
struct Walk<'a> {
    pending: Vec<&'a Selector>,
    visited: HashSet<*const Vec<CompoundSelector>>,
}

impl AttributeDependencies {
    pub(super) fn for_rules(rules: &[Rule]) -> Self {
        let mut dependencies = Self::default();
        let mut walk = Walk::default();
        for rule in rules {
            dependencies.include_rule(rule, &mut walk);
        }
        dependencies
    }

    pub(super) fn for_indices(rules: &[Rule], indices: &[usize]) -> Self {
        let mut dependencies = Self::default();
        let mut walk = Walk::default();
        for &index in indices {
            dependencies.include_rule(&rules[index], &mut walk);
        }
        dependencies
    }

    fn include_rule<'a>(&mut self, rule: &'a Rule, walk: &mut Walk<'a>) {
        walk.pending.push(&rule.selector);
        walk.pending.extend(&rule.host_condition);
        walk.pending.extend(&rule.slotted_origin);
        walk.pending
            .extend(rule.part.iter().map(|part| &part.origin));
        for scope in &rule.css_scopes {
            walk.pending
                .extend(scope.start.iter().chain(scope.end.iter()).flatten());
        }
        while let Some(selector) = walk.pending.pop() {
            if !walk.visited.insert(Rc::as_ptr(&selector.compounds)) {
                continue;
            }
            for compound in selector.compounds.iter() {
                if let Some(id) = compound.id.as_ref().filter(|id| !self.ids.contains(*id)) {
                    self.ids.insert(id.clone());
                }
                for class in &compound.classes {
                    if !self.classes.contains(class) {
                        self.classes.insert(class.clone());
                    }
                }
                for attribute in &compound.attributes {
                    self.class_attribute |= attribute.name.eq_ignore_ascii_case("class");
                    self.id_attribute |= attribute.name.eq_ignore_ascii_case("id");
                    self.style_attribute |= attribute.name.eq_ignore_ascii_case("style");
                }
                for functional in &compound.functional {
                    walk.pending.extend(&functional.selectors);
                }
                for relatives in &compound.has {
                    walk.pending
                        .extend(relatives.iter().map(|relative| &relative.selector));
                }
                for nth in &compound.nth {
                    walk.pending.extend(&nth.filter);
                }
            }
        }
    }

    pub(super) fn may_change(&self, name: &str, old: &str, next: &str) -> bool {
        match name {
            "class" => {
                self.class_attribute
                    || old.split_ascii_whitespace().any(|class| {
                        !next.split_ascii_whitespace().any(|new| new == class)
                            && self.classes.contains(class)
                    })
                    || next.split_ascii_whitespace().any(|class| {
                        !old.split_ascii_whitespace()
                            .any(|previous| previous == class)
                            && self.classes.contains(class)
                    })
            }
            "id" => {
                self.id_attribute
                    || old != next && (self.ids.contains(old) || self.ids.contains(next))
            }
            "style" => self.style_attribute,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::css::media::MediaEnvironment;
    use crate::engine::css::stylesheet::parse_stylesheet;

    #[test]
    fn selector_dependencies_include_nested_and_scoped_conditions() {
        let mut rules = Vec::new();
        parse_stylesheet(
            ".active a{opacity:.5} :is(.selected,#chosen){color:red} main:has(.child){color:blue} @scope (.scoped) {a{color:green}}",
            "",
            MediaEnvironment::new(800.0, 600.0, 1.0, false),
            &mut 0,
            &mut rules,
            RuleScope::Document,
        );
        let dependencies = AttributeDependencies::for_rules(&rules);
        assert!(!dependencies.may_change("class", "noise1", "noise2"));
        assert!(dependencies.may_change("class", "noise", "active"));
        assert!(dependencies.may_change("class", "child", "noise"));
        assert!(dependencies.may_change("class", "noise", "scoped"));
        assert!(dependencies.may_change("id", "noise", "chosen"));
        assert!(!dependencies.may_change("id", "noise", "other"));
        assert!(!dependencies.may_change("class", "active selected", "selected active"));
    }

    #[test]
    fn attribute_selectors_preserve_presence_and_value_dependencies() {
        let mut rules = Vec::new();
        parse_stylesheet(
            "[class*=needle] a{color:red} [id]{color:blue}",
            "",
            MediaEnvironment::new(800.0, 600.0, 1.0, false),
            &mut 0,
            &mut rules,
            RuleScope::Document,
        );
        let dependencies = AttributeDependencies::for_rules(&rules);
        assert!(dependencies.may_change("class", "", ""));
        assert!(dependencies.may_change("id", "", ""));
    }
}
