//! Bounded restyle scopes for `class` attribute changes.
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
// https://drafts.csswg.org/selectors-4/#combinators
use super::subject_keys::{KeyIndex, SubjectKey};
use super::*;
use crate::engine::css::selector_model::{Combinator, CompoundSelector};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Positions in which rules use one class token.
#[derive(Debug, Default)]
struct ClassUse {
    subject: bool,
    /// Subject keys of rules that use the token in an ancestor compound.
    descendant_subjects: Vec<SubjectKey>,
    /// Depths below the element of keyless subjects reached only through child combinators.
    keyless_child_depths: Vec<usize>,
    /// The token's effect is not bounded by the element and its descendants.
    parent_scope: bool,
}

#[derive(Debug, Default)]
pub(super) struct ClassInvalidation {
    uses: HashMap<String, ClassUse>,
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
            let mut tokens = compound.classes.iter().collect::<Vec<_>>();
            let mut nested_complex = Vec::new();
            nested_simple_classes(compound, &mut tokens, &mut nested_complex);
            let record = |class_use: &mut ClassUse| {
                if siblings {
                    class_use.parent_scope = true;
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
                record(self.uses.entry(token.to_ascii_lowercase()).or_default());
            }
            if simple_class_attribute(compound) {
                record(&mut self.any_token);
            }
            self.unbounded |= nested_complex_class_attribute(compound);
            for token in nested_complex {
                self.uses
                    .entry(token.to_ascii_lowercase())
                    .or_default()
                    .parent_scope = true;
            }
        }
    }

    /// Returns `None` when the change needs the existing parent-subtree scope.
    fn roots(&self, node: &NodeRef, tokens: &[String]) -> Option<ClassChangeRoots> {
        if self.unbounded {
            return None;
        }
        let mut keys = HashSet::new();
        let mut depths = HashSet::new();
        let token_uses = tokens
            .iter()
            .filter_map(|token| self.uses.get(&token.to_ascii_lowercase()));
        for class_use in token_uses.chain([&self.any_token]) {
            if class_use.parent_scope {
                return None;
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
            return Some(roots);
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
            return Some(roots);
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
        Some(roots)
    }
}

/// Classes inside `:is()`, `:where()` and `:not()` arguments that are single compounds describe
/// the same element as `compound`. Any other nesting (complex arguments, `:has()`, `of S`) is
/// collected as unbounded.
fn nested_simple_classes<'a>(
    compound: &'a CompoundSelector,
    simple: &mut Vec<&'a String>,
    complex: &mut Vec<String>,
) {
    for functional in &compound.functional {
        for selector in &functional.selectors {
            if selector.compounds.len() == 1 {
                simple.extend(selector.compounds[0].classes.iter());
                nested_simple_classes(&selector.compounds[0], simple, complex);
            } else {
                collect_all_classes(selector, complex);
            }
        }
    }
    for relatives in &compound.has {
        for relative in relatives {
            collect_all_classes(&relative.selector, complex);
        }
    }
    for nth in &compound.nth {
        for selector in &nth.filter {
            collect_all_classes(selector, complex);
        }
    }
}

fn is_class_attribute(compound: &CompoundSelector) -> bool {
    compound
        .attributes
        .iter()
        .any(|attribute| attribute.name.eq_ignore_ascii_case("class"))
}

/// A `[class]` selector on this element, directly or in a single-compound nested argument.
fn simple_class_attribute(compound: &CompoundSelector) -> bool {
    is_class_attribute(compound)
        || compound.functional.iter().any(|functional| {
            functional.selectors.iter().any(|selector| {
                selector.compounds.len() == 1 && simple_class_attribute(&selector.compounds[0])
            })
        })
}

/// A `[class]` selector inside a nested complex selector, `:has()` or `of S`.
fn nested_complex_class_attribute(compound: &CompoundSelector) -> bool {
    let anywhere = |selector: &Selector| {
        selector.compounds.iter().any(|compound| {
            simple_class_attribute(compound) || nested_complex_class_attribute(compound)
        })
    };
    compound.functional.iter().any(|functional| {
        functional.selectors.iter().any(|selector| {
            if selector.compounds.len() == 1 {
                nested_complex_class_attribute(&selector.compounds[0])
            } else {
                anywhere(selector)
            }
        })
    }) || compound
        .has
        .iter()
        .flatten()
        .any(|relative| anywhere(&relative.selector))
        || compound
            .nth
            .iter()
            .flat_map(|nth| &nth.filter)
            .any(anywhere)
}

fn collect_all_classes(selector: &Selector, classes: &mut Vec<String>) {
    for compound in selector.compounds.iter() {
        classes.extend(compound.classes.iter().cloned());
        for functional in &compound.functional {
            for nested in &functional.selectors {
                collect_all_classes(nested, classes);
            }
        }
        for relatives in &compound.has {
            for relative in relatives {
                collect_all_classes(&relative.selector, classes);
            }
        }
        for nth in &compound.nth {
            for nested in &nth.filter {
                collect_all_classes(nested, classes);
            }
        }
    }
}

impl StyleSet {
    /// Converts class changes into subtree roots and early-cutoff element roots. A change that
    /// cannot be bounded keeps its parent subtree (or the document for the root element).
    pub(crate) fn resolve_class_changes(
        &self,
        dom: &Dom,
        changes: &[crate::engine::invalidation::ClassChange],
        subtrees: &mut Vec<NodeRef>,
        elements: &mut Vec<NodeRef>,
    ) {
        for change in changes {
            let Some(node) = dom.find_node(change.node) else {
                continue;
            };
            match self
                .compiled
                .class_invalidation
                .roots(&node, &change.tokens)
            {
                Some(roots) => {
                    subtrees.extend(roots.subtrees);
                    elements.extend(roots.elements);
                }
                None => subtrees.push(
                    node.shadow_including_parent()
                        .unwrap_or_else(|| dom.document.clone()),
                ),
            }
        }
    }
}
