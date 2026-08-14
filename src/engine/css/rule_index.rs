//! Rightmost-compound candidate index for selector matching.

use super::*;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub(super) struct RuleIndex {
    universal: Vec<usize>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
}

impl RuleIndex {
    pub(super) fn new(rules: &[Rule]) -> Self {
        let mut index = Self::default();
        for (rule_index, rule) in rules.iter().enumerate() {
            let Some(target) = rule.selector.compounds.last() else {
                continue;
            };
            if let Some(id) = target.id.as_ref() {
                index.by_id.entry(id.clone()).or_default().push(rule_index);
            } else if let Some(class) = target.classes.first() {
                index
                    .by_class
                    .entry(class.clone())
                    .or_default()
                    .push(rule_index);
            } else if let Some(tag) = target.tag.as_ref() {
                index
                    .by_tag
                    .entry(tag.clone())
                    .or_default()
                    .push(rule_index);
            } else if target.requires_link {
                index
                    .by_tag
                    .entry("a".to_string())
                    .or_default()
                    .push(rule_index);
            } else {
                index.universal.push(rule_index);
            }
        }
        index
    }

    pub(super) fn candidates(&self, node: &NodeRef) -> Vec<usize> {
        let mut candidates = self.universal.clone();
        if let Some(id) = node.attr("id")
            && let Some(rules) = self.by_id.get(&id)
        {
            candidates.extend(rules);
        }
        if let Some(tag) = node.tag_name()
            && let Some(rules) = self.by_tag.get(tag)
        {
            candidates.extend(rules);
        }
        if let Some(classes) = node.attr("class") {
            let mut seen = HashSet::new();
            for class in classes.split_ascii_whitespace() {
                if seen.insert(class)
                    && let Some(rules) = self.by_class.get(class)
                {
                    candidates.extend(rules);
                }
            }
        }
        candidates
    }
}
