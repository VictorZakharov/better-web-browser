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
// element alone, since only `attr()` and its own style can read it. These are the descendant
// invalidation sets of Blink's style invalidation.
// https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/css/style-invalidation.md
// https://drafts.csswg.org/selectors-4/#combinators
use super::subject_keys::{KeyIndex, SubjectKey, ascii_lowercase};
use super::*;
use crate::engine::css::cascade::InvalidationFallbacks;
use crate::engine::css::selector_model::{Combinator, CompoundSelector};
use std::borrow::Cow;
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
    /// Positions of attribute selectors other than `[class]`, by ASCII-lowercase name.
    attribute_uses: HashMap<String, ClassUse>,
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
            for name in names {
                record(use_of(&mut self.attribute_uses, &name));
            }
            for name in nested_complex_names {
                let class_use = use_of(&mut self.attribute_uses, &name);
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
        let attribute_uses = attributes
            .iter()
            .filter_map(|name| self.attribute_uses.get(name));
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

/// What a change names in selectors: a class token or an attribute name.
#[derive(Clone, Copy)]
enum Feature {
    Class,
    /// Attribute selectors other than `[class]`, which every token change affects.
    Attribute,
}

impl Feature {
    /// Borrowed from the selector where possible: most rules repeat names already indexed.
    fn of(self, compound: &CompoundSelector) -> Vec<Cow<'_, str>> {
        match self {
            Self::Class => compound.classes.iter().map(|class| class.into()).collect(),
            Self::Attribute => compound
                .attributes
                .iter()
                .map(|attribute| ascii_lowercase(&attribute.name))
                .filter(|name| name != "class")
                .collect(),
        }
    }
}

/// The uses recorded for `name`, copying the name only when it is new.
fn use_of<'a>(uses: &'a mut HashMap<String, ClassUse>, name: &str) -> &'a mut ClassUse {
    if !uses.contains_key(name) {
        uses.insert(name.to_owned(), ClassUse::default());
    }
    uses.get_mut(name).expect("inserted above")
}

/// Features inside `:is()`, `:where()` and `:not()` arguments that are single compounds describe
/// the same element as `compound`. Any other nesting (complex arguments, `:has()`, `of S`) is
/// collected as unbounded.
fn nested_simple_features<'a>(
    compound: &'a CompoundSelector,
    feature: Feature,
    simple: &mut Vec<Cow<'a, str>>,
    complex: &mut Vec<Cow<'a, str>>,
) {
    for functional in &compound.functional {
        for selector in &functional.selectors {
            if selector.compounds.len() == 1 {
                simple.extend(feature.of(&selector.compounds[0]));
                nested_simple_features(&selector.compounds[0], feature, simple, complex);
            } else {
                collect_all_features(selector, feature, complex);
            }
        }
    }
    for relatives in &compound.has {
        for relative in relatives {
            collect_all_features(&relative.selector, feature, complex);
        }
    }
    for nth in &compound.nth {
        for selector in &nth.filter {
            collect_all_features(selector, feature, complex);
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

fn collect_all_features<'a>(
    selector: &'a Selector,
    feature: Feature,
    found: &mut Vec<Cow<'a, str>>,
) {
    for compound in selector.compounds.iter() {
        found.extend(feature.of(compound));
        for functional in &compound.functional {
            for nested in &functional.selectors {
                collect_all_features(nested, feature, found);
            }
        }
        for relatives in &compound.has {
            for relative in relatives {
                collect_all_features(&relative.selector, feature, found);
            }
        }
        for nth in &compound.nth {
            for nested in &nth.filter {
                collect_all_features(nested, feature, found);
            }
        }
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
