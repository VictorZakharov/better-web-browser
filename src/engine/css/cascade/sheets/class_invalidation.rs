//! Bounded restyle scopes for `class` attribute changes, and for attributes that only
//! selectors read (`invalidation::selector_only_attribute`).
//
// A class token can only change matching for the element that carries it, for its descendants
// through descendant/child combinators, or for later siblings through sibling combinators.
// Restyling the element's parent subtree covers all three, but for the root element that is the
// whole document. When every rule that mentions a changed token uses it in the subject compound
// or in an ancestor compound, the affected elements are the element itself plus descendants that
// can match the subjects of those ancestor-position rules. Those are restyled as element roots:
// recalculated alone, with their subtree following only when their own style changes.
// A keyless subject (`.a > :not(.b)`) reached only through child combinators is bounded by its
// depth below the element, so only the elements at that depth are restyled.
// Sibling combinators, `:has()` and nested complex selectors keep the parent-subtree scope.
// An attribute selector on `class` (`[class*=x]`) can match differently after any token change,
// so it counts as a use of every changed token at its position; inside a complex nested
// selector it keeps the parent-subtree scope for every change.
// An attribute name is indexed like a class token: `[data-state=open] h2` bounds a `data-state`
// change to the element and its `h2` descendants, and a name no selector mentions restyles the
// element alone, since only `attr()` and its own style can read it. Entering or leaving `:hover`
// is indexed the same way under `invalidation::HOVER_STATE`. These are the descendant
// invalidation sets of Blink's style invalidation.
// https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/css/style-invalidation.md
// https://drafts.csswg.org/selectors-4/#combinators
use super::subject_keys::{KeyIndex, SubjectKey, ascii_lowercase};
use super::*;
use crate::engine::css::cascade::InvalidationFallbacks;
use crate::engine::css::selector_model::Combinator;
use std::collections::{HashMap, HashSet};

mod features;
#[cfg(test)]
mod tests;

use features::{
    Feature, guarded_use, nested_complex_class_attribute, nested_simple_features,
    simple_class_attribute, use_of,
};

/// Positions in which rules use one class token.
#[derive(Debug, Default)]
struct ClassUse {
    subject: bool,
    /// Subject keys of rules that use the token in an ancestor compound.
    descendant_subjects: Vec<SubjectKey>,
    /// Depths below the element of keyless subjects reached only through child combinators.
    keyless_child_depths: Vec<usize>,
    /// Why the token's effect is not bounded by the element and its descendants.
    parent_scope: Option<Fallback>,
}

/// Why a change keeps the parent-subtree scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fallback {
    Siblings,
    Nested,
    Unbounded,
}

#[derive(Debug, Default)]
pub(super) struct ClassInvalidation {
    uses: HashMap<String, ClassUse>,
    /// Positions of attribute selectors other than `[class]` (and of `:hover`), by ASCII-lowercase
    /// name and then by the key of the compound that carries them.
    attribute_uses: HashMap<String, HashMap<SubjectKey, ClassUse>>,
    /// Positions of `[class]` attribute selectors, which apply to every token.
    any_token: ClassUse,
    /// Nested complex `[class]` selectors or scoped/cross-tree rules: no token can be bounded.
    unbounded: bool,
}

/// Roots for one class change: subtrees to recalculate fully and element roots to recalculate
/// with an early cutoff.
#[derive(Debug, Default)]
struct ClassChangeRoots {
    subtrees: Vec<NodeRef>,
    elements: Vec<NodeRef>,
}

impl ClassInvalidation {
    pub(super) fn for_rules(rules: &[Rule], cross_tree_or_scoped: bool) -> Self {
        let mut index = Self {
            unbounded: cross_tree_or_scoped,
            ..Self::default()
        };
        for rule in rules {
            index.include(&rule.selector);
        }
        index
    }

    fn include(&mut self, selector: &Selector) {
        let last = selector.compounds.len().saturating_sub(1);
        let subject = &selector.compounds[last];
        for (position, compound) in selector.compounds.iter().enumerate() {
            let siblings = selector.combinators[position..].iter().any(|combinator| {
                matches!(
                    combinator,
                    Combinator::AdjacentSibling | Combinator::GeneralSibling
                )
            });
            let mut tokens = Feature::Class.of(compound);
            let mut nested_complex = Vec::new();
            nested_simple_features(compound, Feature::Class, &mut tokens, &mut nested_complex);
            let mut names = Feature::Attribute.of(compound);
            let mut nested_complex_names = Vec::new();
            nested_simple_features(
                compound,
                Feature::Attribute,
                &mut names,
                &mut nested_complex_names,
            );
            let record = |class_use: &mut ClassUse| {
                if siblings {
                    class_use.parent_scope.get_or_insert(Fallback::Siblings);
                } else if position == last {
                    class_use.subject = true;
                } else {
                    let key = SubjectKey::of(subject);
                    let children_only = selector.combinators[position..]
                        .iter()
                        .all(|combinator| matches!(combinator, Combinator::Child));
                    if key.matches_any() && children_only {
                        class_use.keyless_child_depths.push(last - position);
                    } else {
                        class_use.descendant_subjects.push(key);
                    }
                }
            };
            for token in tokens {
                record(use_of(&mut self.uses, &ascii_lowercase(&token)));
            }
            // An element that cannot match the compound's tag, ID and classes cannot change
            // whether it matches, whatever its attribute or `:hover` state.
            let guard = SubjectKey::of(compound);
            for name in names {
                record(guarded_use(&mut self.attribute_uses, &name, &guard));
            }
            for name in nested_complex_names {
                let class_use =
                    guarded_use(&mut self.attribute_uses, &name, &SubjectKey::default());
                class_use.parent_scope.get_or_insert(Fallback::Nested);
            }
            if simple_class_attribute(compound) {
                record(&mut self.any_token);
            }
            self.unbounded |= nested_complex_class_attribute(compound);
            for token in nested_complex {
                let class_use = use_of(&mut self.uses, &ascii_lowercase(&token));
                class_use.parent_scope.get_or_insert(Fallback::Nested);
            }
        }
    }

    /// Returns why the change needs the existing parent-subtree scope instead, if it does.
    fn roots(
        &self,
        node: &NodeRef,
        tokens: &[String],
        attributes: &[String],
    ) -> Result<ClassChangeRoots, Fallback> {
        if self.unbounded {
            return Err(Fallback::Unbounded);
        }
        let mut keys = HashSet::new();
        let mut depths = HashSet::new();
        let token_uses = tokens
            .iter()
            .filter_map(|token| self.uses.get(&*ascii_lowercase(token)));
        let class_attribute = (!tokens.is_empty()).then_some(&self.any_token);
        // Guards read the element's classes, so a class change in the same batch skips them.
        let classes_changed = !tokens.is_empty();
        let attribute_uses = attributes
            .iter()
            .filter_map(|name| self.attribute_uses.get(name))
            .flatten()
            .filter(|(guard, _)| classes_changed || guard.may_match(node))
            .map(|(_, class_use)| class_use);
        for class_use in token_uses.chain(class_attribute).chain(attribute_uses) {
            if let Some(fallback) = class_use.parent_scope {
                return Err(fallback);
            }
            keys.extend(class_use.descendant_subjects.iter());
            depths.extend(class_use.keyless_child_depths.iter().copied());
        }
        // `attr(class)` and presentational state still read the element's own attribute.
        let mut roots = ClassChangeRoots {
            elements: vec![node.clone()],
            ..ClassChangeRoots::default()
        };
        if keys.iter().any(|key| key.matches_any()) {
            roots.subtrees.push(node.clone());
            return Ok(roots);
        }
        let mut level = vec![node.clone()];
        for depth in 1..=depths.iter().copied().max().unwrap_or(0) {
            level = level
                .iter()
                .flat_map(|parent| parent.children.borrow().clone())
                .filter(|child| child.element().is_some())
                .collect();
            if depths.contains(&depth) {
                roots.elements.extend(level.iter().cloned());
            }
        }
        if keys.is_empty() {
            return Ok(roots);
        }
        let index = KeyIndex::new(keys);
        let at_depth = roots
            .elements
            .iter()
            .map(|element| element.id())
            .collect::<HashSet<_>>();
        roots
            .elements
            .extend(Node::descendants(node).skip(1).filter(|descendant| {
                !at_depth.contains(&descendant.id()) && index.may_match(descendant)
            }));
        Ok(roots)
    }
}

impl StyleSet {
    /// Converts class and attribute changes into subtree roots and early-cutoff element roots. A change that
    /// cannot be bounded keeps its parent subtree (or the document for the root element); the
    /// result counts those and the whole subtrees of keyless descendant subjects.
    pub(crate) fn resolve_attribute_changes(
        &self,
        dom: &Dom,
        changes: &[crate::engine::invalidation::AttributeChange],
        subtrees: &mut Vec<NodeRef>,
        elements: &mut Vec<NodeRef>,
    ) -> InvalidationFallbacks {
        let mut fallbacks = InvalidationFallbacks::default();
        let ids = changes.iter().map(|change| change.node).collect::<Vec<_>>();
        let nodes = dom.find_nodes(&ids);
        for change in changes {
            let Some(node) = nodes.get(&change.node).cloned() else {
                continue;
            };
            match self
                .compiled
                .class_invalidation
                .roots(&node, &change.tokens, &change.attributes)
            {
                Ok(roots) => {
                    fallbacks.keyless_descendant += roots.subtrees.len() as u64;
                    subtrees.extend(roots.subtrees);
                    elements.extend(roots.elements);
                }
                Err(fallback) => {
                    *match fallback {
                        Fallback::Siblings => &mut fallbacks.sibling_combinator,
                        Fallback::Nested => &mut fallbacks.nested_selector,
                        Fallback::Unbounded => &mut fallbacks.unbounded_rules,
                    } += 1;
                    subtrees.push(
                        node.shadow_including_parent()
                            .unwrap_or_else(|| dom.document.clone()),
                    );
                }
            }
        }
        fallbacks
    }
}
