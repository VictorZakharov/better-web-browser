//! Restyle only the subjects of added or removed rules when a stylesheet set changes.
//
// A rule can only change the computed style of elements its rightmost compound can match (and,
// through inheritance and generated content, their subtrees). When two compiled rule sets share
// every other rule's parsed payload, cascade layer and relative order, the matched rules of an
// element that no changed rule can target are identical, so its computed style is too. Changes
// to keyframes, font faces, the media environment, or cross-tree and scoped rules keep the
// whole-document rebuild. This mirrors stylesheet invalidation in engines that compute rule-set
// diffs instead of restyling the document for every inserted or removed sheet.
// https://drafts.csswg.org/cssom/#add-a-css-style-sheet
use super::*;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Keys an element must carry to be a subject of at least one changed rule.
#[derive(Default)]
struct SubjectKeys {
    ids: HashSet<String>,
    classes: HashSet<String>,
    tags: HashSet<String>,
    attributes: HashSet<String>,
}

impl SubjectKeys {
    /// Adds the rule's least selective positive key, or fails for a keyless subject.
    fn include(&mut self, rule: &Rule) -> Option<()> {
        let subject = rule.selector.compounds.last()?;
        // ASCII case-insensitive keys are a superset of both standards and quirks matching.
        if let Some(id) = &subject.id {
            self.ids.insert(id.to_ascii_lowercase());
        } else if let Some(class) = subject.classes.first() {
            self.classes.insert(class.to_ascii_lowercase());
        } else if let Some(tag) = &subject.tag {
            self.tags.insert(tag.to_ascii_lowercase());
        } else if let Some(attribute) = subject.attributes.first() {
            self.attributes.insert(attribute.name.to_ascii_lowercase());
        } else if subject.requires_link {
            self.tags.insert("a".into());
        } else {
            return None;
        }
        Some(())
    }

    fn may_match(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        if self
            .tags
            .contains(&element.name.local.as_ref().to_ascii_lowercase())
        {
            return true;
        }
        element.attrs.borrow().iter().any(|attribute| {
            let name = attribute.name.local.as_ref().to_ascii_lowercase();
            let value = attribute.value.as_ref();
            self.attributes.contains(&name)
                || (name == "id" && self.ids.contains(&value.to_ascii_lowercase()))
                || (name == "class"
                    && value
                        .split_ascii_whitespace()
                        .any(|class| self.classes.contains(&class.to_ascii_lowercase())))
        })
    }
}

pub(super) struct RuleDiff {
    /// Elements whose computed style can differ between the two rule sets.
    pub(super) subjects: Vec<NodeRef>,
    /// A changed rule contains `:has()`, so remembered anchor results no longer line up.
    pub(super) relational: bool,
}

/// Compares two rule sets, or returns `None` when their difference is not limited to whole
/// rules that can be keyed to their subjects.
pub(super) fn changed_rule_subjects(
    old: &CompiledRules,
    new: &CompiledRules,
    document: &NodeRef,
) -> Option<RuleDiff> {
    if old.environment != new.environment
        || old.cross_tree_or_scoped
        || new.cross_tree_or_scoped
        || old.keyframes != new.keyframes
        || old.font_faces != new.font_faces
    {
        return None;
    }
    let old_rules = positions(&old.rules)?;
    let new_rules = positions(&new.rules)?;
    let mut keys = SubjectKeys::default();
    let mut relational = false;
    let mut previous = None;
    for rule in &new.rules {
        let Some(&index) = old_rules.get(&identity(rule)) else {
            keys.include(rule)?;
            relational |= super::has_anchors::HasAnchors::is_relational(&rule.selector);
            continue;
        };
        let shared = &old.rules[index];
        // Unchanged rules must keep their cascade layer and relative source order.
        if shared.layer_rank != rule.layer_rank
            || shared.layer != rule.layer
            || previous.is_some_and(|previous| previous > index)
        {
            return None;
        }
        previous = Some(index);
    }
    for rule in old
        .rules
        .iter()
        .filter(|rule| !new_rules.contains_key(&identity(rule)))
    {
        keys.include(rule)?;
        relational |= super::has_anchors::HasAnchors::is_relational(&rule.selector);
    }
    Some(RuleDiff {
        subjects: Node::composed_descendants(document)
            .filter(|node| keys.may_match(node))
            .collect(),
        relational,
    })
}

fn identity(rule: &Rule) -> *const crate::engine::css::stylesheet::RuleData {
    rule.data_ptr()
}

/// Maps each parsed rule payload to its position; a payload used twice is ambiguous.
fn positions(
    rules: &[Rule],
) -> Option<HashMap<*const crate::engine::css::stylesheet::RuleData, usize>> {
    let mut positions = HashMap::with_capacity(rules.len());
    for (index, rule) in rules.iter().enumerate() {
        if positions.insert(identity(rule), index).is_some() {
            return None;
        }
    }
    Some(positions)
}
