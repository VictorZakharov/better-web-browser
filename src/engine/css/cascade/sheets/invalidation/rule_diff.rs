//! Restyle only the subjects of added or removed rules when a stylesheet set changes.
//
// A rule can only change the computed style of elements its rightmost compound can match (and,
// through inheritance and generated content, their subtrees). When two compiled rule sets share
// every other rule's parsed payload, cascade layer and relative order, the matched rules of an
// element that no changed rule can target are identical, so its computed style is too. Computed
// styles name animations but do not contain their keyframes, which animation discovery looks up
// in the new rule set, so `@keyframes` changes alone restyle nothing. Changes to font faces, the
// media environment, or cross-tree and scoped rules keep the whole-document rebuild. This
// mirrors stylesheet invalidation in engines that compute rule-set diffs instead of restyling
// the document for every inserted or removed sheet.
// https://drafts.csswg.org/cssom/#add-a-css-style-sheet
use super::*;
use crate::engine::css::rule_index::RuleIndex;
use crate::engine::dom::node::NodeMap;
use std::cell::OnceCell;
use std::collections::HashSet;

#[cfg(test)]
mod tests;

/// Keys of one kind of affected element.
#[derive(Default)]
struct KeySet {
    ids: HashSet<String>,
    classes: HashSet<String>,
    tags: HashSet<String>,
    attributes: HashSet<String>,
}

impl KeySet {
    fn is_empty(&self) -> bool {
        self.ids.is_empty()
            && self.classes.is_empty()
            && self.tags.is_empty()
            && self.attributes.is_empty()
    }

    /// Called for every element of the document, so names that are already lowercase, as
    /// parsed HTML names and most ids and classes are, are looked up without a copy.
    fn may_match(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        if !self.tags.is_empty() && self.tags.contains(&*lowercase(&element.name.local)) {
            return true;
        }
        if self.attributes.is_empty() && self.ids.is_empty() && self.classes.is_empty() {
            return false;
        }
        element.attrs.borrow().iter().any(|attribute| {
            let name = lowercase(&attribute.name.local);
            let value = attribute.value.as_ref();
            self.attributes.contains(&*name)
                || (&*name == "id" && !self.ids.is_empty() && self.ids.contains(&*lowercase(value)))
                || (&*name == "class"
                    && !self.classes.is_empty()
                    && value
                        .split_ascii_whitespace()
                        .any(|class| self.classes.contains(&*lowercase(class))))
        })
    }
}

fn lowercase(value: &str) -> std::borrow::Cow<'_, str> {
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        value.to_ascii_lowercase().into()
    } else {
        value.into()
    }
}

/// Keys of the elements whose style a changed rule may change: its subjects themselves, or,
/// for a subject without a key, ancestors whose subtrees contain every possible subject.
#[derive(Default)]
struct SubjectKeys {
    subjects: KeySet,
    ancestors: KeySet,
}

impl SubjectKeys {
    fn include(&mut self, rule: &Rule) -> Option<()> {
        self.include_selector(&rule.selector, true)
    }

    /// The subject compound's own key, or else the nearest compound to its left that has
    /// one when only descendant and child combinators separate them: every subject then lies
    /// in the subtree of an element carrying that key, and that subtree is restyled whole.
    fn include_selector(&mut self, selector: &Selector, subject: bool) -> Option<()> {
        let last = selector.compounds.len().checked_sub(1)?;
        for index in (0..=last).rev() {
            if self
                .include_compound(&selector.compounds[index], subject && index == last)
                .is_some()
            {
                return Some(());
            }
            let combinator = selector.combinators.get(index.checked_sub(1)?)?;
            if !matches!(combinator, Combinator::Descendant | Combinator::Child) {
                return None;
            }
        }
        None
    }

    /// Adds the compound's least selective positive key, or fails for a keyless compound.
    fn include_compound(&mut self, compound: &CompoundSelector, subject: bool) -> Option<()> {
        let keys = if subject {
            &mut self.subjects
        } else {
            &mut self.ancestors
        };
        // ASCII case-insensitive keys are a superset of both standards and quirks matching.
        if let Some(id) = &compound.id {
            keys.ids.insert(id.to_ascii_lowercase());
        } else if let Some(class) = compound.classes.first() {
            keys.classes.insert(class.to_ascii_lowercase());
        } else if let Some(tag) = &compound.tag {
            keys.tags.insert(tag.to_ascii_lowercase());
        } else if let Some(attribute) = compound.attributes.first() {
            keys.attributes.insert(attribute.name.to_ascii_lowercase());
        } else if compound.requires_link {
            keys.tags.insert("a".into());
        } else {
            // `:is()` and `:where()` match what one of their arguments matches.
            let alternatives = compound.functional.iter().find(|functional| {
                matches!(
                    functional.kind,
                    FunctionalSelectorKind::Is | FunctionalSelectorKind::Where
                )
            })?;
            for alternative in &alternatives.selectors {
                self.include_selector(alternative, subject)?;
            }
        }
        Some(())
    }
}

pub(super) struct RuleDiff {
    /// Elements a changed rule may match. Only their own style (with generated content) can
    /// change, and their subtrees through inheritance, so they restyle with an early cutoff.
    pub(super) subjects: Vec<NodeRef>,
    /// Elements whose whole subtrees a changed rule with a keyless subject may match.
    pub(super) subtrees: Vec<NodeRef>,
    /// A changed rule contains `:has()`, so remembered anchor results no longer line up.
    pub(super) relational: bool,
    /// The added and removed rules.
    pub(super) changed: Vec<Rule>,
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
        || old.font_faces != new.font_faces
    {
        return None;
    }
    let old_rules = positions(&old.rules)?;
    let new_rules = positions(&new.rules)?;
    let mut keys = SubjectKeys::default();
    let mut relational = false;
    let mut changed = Vec::new();
    let mut previous = None;
    for rule in &new.rules {
        let Some(&index) = old_rules.get(&identity(rule)) else {
            keys.include(rule)?;
            relational |= super::has_anchors::HasAnchors::is_relational(&rule.selector);
            changed.push(rule.clone());
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
        changed.push(rule.clone());
    }
    let (mut subjects, mut subtrees) = (Vec::new(), Vec::new());
    if !keys.ancestors.is_empty() || !keys.subjects.is_empty() {
        for node in Node::composed_descendants(document) {
            if keys.ancestors.may_match(&node) {
                subtrees.push(node);
            } else if keys.subjects.may_match(&node) {
                subjects.push(node);
            }
        }
    }
    Some(RuleDiff {
        subjects,
        subtrees,
        relational,
        changed,
    })
}

impl StyleSet {
    /// Keeps the key-matched subjects that a changed rule matches in full. A subject key alone
    /// names every `<a>` for a rule like `.ve-activated #catlinks a`, which matches none of them
    /// until an ancestor gains its class.
    ///
    /// Removed rules are matched against the current document too. Since the previous cascade,
    /// every DOM change has its own invalidation root and every class change is resolved against
    /// both rule sets, so outside those roots a rule matched then exactly when it matches now.
    pub(super) fn subjects_matched_by(
        &self,
        changed: &[Rule],
        subjects: Vec<NodeRef>,
    ) -> Vec<NodeRef> {
        let index = RuleIndex::new(changed);
        let mut pseudos = Vec::new();
        for rule in changed {
            if !pseudos.contains(&rule.pseudo) {
                pseudos.push(rule.pseudo);
            }
        }
        subjects
            .into_iter()
            .filter(|node| {
                let tree_root = Node::tree_root(node);
                let ancestors = OnceCell::new();
                pseudos.iter().any(|&pseudo| {
                    index.candidates(node, pseudo).into_iter().any(|position| {
                        self.rule_matches(&changed[position], node, pseudo, &tree_root, &ancestors)
                    })
                })
            })
            .collect()
    }
}

fn identity(rule: &Rule) -> *const crate::engine::css::stylesheet::RuleData {
    rule.data_ptr()
}

/// Maps each parsed rule payload to its position; a payload used twice is ambiguous.
fn positions(
    rules: &[Rule],
) -> Option<NodeMap<*const crate::engine::css::stylesheet::RuleData, usize>> {
    let mut positions = NodeMap::with_capacity_and_hasher(rules.len(), Default::default());
    for (index, rule) in rules.iter().enumerate() {
        if positions.insert(identity(rule), index).is_some() {
            return None;
        }
    }
    Some(positions)
}
