//! Bounded restyle scopes for `class` attribute changes.
//
// A class token can only change matching for the element that carries it, for its descendants
// through descendant/child combinators, or for later siblings through sibling combinators.
// Restyling the element's parent subtree covers all three, but for the root element that is the
// whole document. When every rule that mentions a changed token uses it in the subject compound
// or in an ancestor compound, the affected elements are the element itself plus descendants that
// can match the subjects of those ancestor-position rules. Those are restyled as element roots:
// recalculated alone, with their subtree following only when their own style changes.
// Sibling combinators, `:has()` and nested complex selectors keep the parent-subtree scope.
// An attribute selector on `class` (`[class*=x]`) can match differently after any token change,
// so it counts as a use of every changed token at its position; inside a complex nested
// selector it keeps the parent-subtree scope for every change.
// https://drafts.csswg.org/selectors-4/#combinators
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
    /// The token's effect is not bounded by the element and its descendants.
    parent_scope: bool,
}

/// Tag, ID and classes of a rule's subject compound. An empty key matches any element.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct SubjectKey {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
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
                    class_use.descendant_subjects.push(SubjectKey::of(subject));
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
        let token_uses = tokens
            .iter()
            .filter_map(|token| self.uses.get(&token.to_ascii_lowercase()));
        for class_use in token_uses.chain([&self.any_token]) {
            if class_use.parent_scope {
                return None;
            }
            keys.extend(class_use.descendant_subjects.iter());
        }
        // `attr(class)` and presentational state still read the element's own attribute.
        let mut roots = ClassChangeRoots {
            elements: vec![node.clone()],
            ..ClassChangeRoots::default()
        };
        if keys.is_empty() {
            return Some(roots);
        }
        if keys.iter().any(|key| key.matches_any()) {
            roots.subtrees.push(node.clone());
            return Some(roots);
        }
        let index = KeyIndex::new(keys);
        roots.elements.extend(
            Node::descendants(node)
                .skip(1)
                .filter(|descendant| index.may_match(descendant)),
        );
        Some(roots)
    }
}

/// Subject keys bucketed by one required feature, so a descendant scan checks only the keys
/// that share a class, ID or tag with each element.
struct KeyIndex<'a> {
    by_class: HashMap<String, Vec<&'a SubjectKey>>,
    by_id: HashMap<String, Vec<&'a SubjectKey>>,
    by_tag: HashMap<String, Vec<&'a SubjectKey>>,
}

impl<'a> KeyIndex<'a> {
    fn new(keys: HashSet<&'a SubjectKey>) -> Self {
        let mut index = Self {
            by_class: HashMap::new(),
            by_id: HashMap::new(),
            by_tag: HashMap::new(),
        };
        for key in keys {
            let (bucket, feature) = if let Some(class) = key.classes.first() {
                (&mut index.by_class, class)
            } else if let Some(id) = &key.id {
                (&mut index.by_id, id)
            } else {
                (
                    &mut index.by_tag,
                    key.tag.as_ref().expect("keyless subjects are handled"),
                )
            };
            bucket
                .entry(feature.to_ascii_lowercase())
                .or_default()
                .push(key);
        }
        index
    }

    fn may_match(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        let check = |keys: Option<&Vec<&SubjectKey>>| {
            keys.is_some_and(|keys| keys.iter().any(|key| key.may_match(node)))
        };
        check(
            self.by_tag
                .get(&element.name.local.as_ref().to_ascii_lowercase()),
        ) || node
            .attr_ref("id")
            .is_some_and(|id| check(self.by_id.get(&id.to_ascii_lowercase())))
            || node.attr_ref("class").is_some_and(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class| check(self.by_class.get(&class.to_ascii_lowercase())))
            })
    }
}

impl SubjectKey {
    fn of(compound: &CompoundSelector) -> Self {
        Self {
            tag: compound.tag.clone().filter(|tag| tag != "*"),
            id: compound.id.clone(),
            classes: compound.classes.clone(),
        }
    }

    fn matches_any(&self) -> bool {
        self.tag.is_none() && self.id.is_none() && self.classes.is_empty()
    }

    // ASCII case-insensitive comparison is a superset of standards and quirks matching.
    fn may_match(&self, node: &NodeRef) -> bool {
        let Some(element) = node.element() else {
            return false;
        };
        self.tag
            .as_deref()
            .is_none_or(|tag| element.name.local.as_ref().eq_ignore_ascii_case(tag))
            && self.id.as_deref().is_none_or(|id| {
                node.attr_ref("id")
                    .is_some_and(|value| value.eq_ignore_ascii_case(id))
            })
            && (self.classes.is_empty()
                || node.attr_ref("class").is_some_and(|value| {
                    self.classes.iter().all(|wanted| {
                        value
                            .split_ascii_whitespace()
                            .any(|class| class.eq_ignore_ascii_case(wanted))
                    })
                }))
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
