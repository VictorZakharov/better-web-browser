//! The class tokens, attribute names and `:hover` state that selectors name, and where.
use super::super::subject_keys::{SubjectKey, ascii_lowercase};
use super::ClassUse;
use crate::engine::css::selector_model::{CompoundSelector, Selector};
use std::borrow::Cow;
use std::collections::HashMap;

/// What a change names in selectors: a class token or an attribute name.
#[derive(Clone, Copy)]
pub(super) enum Feature {
    Class,
    /// Attribute selectors other than `[class]`, which every token change affects.
    Attribute,
}

impl Feature {
    /// Borrowed from the selector where possible: most rules repeat names already indexed.
    pub(super) fn of(self, compound: &CompoundSelector) -> Vec<Cow<'_, str>> {
        match self {
            Self::Class => compound.classes.iter().map(|class| class.into()).collect(),
            Self::Attribute => compound
                .attributes
                .iter()
                .map(|attribute| ascii_lowercase(&attribute.name))
                .filter(|name| name != "class")
                .chain(
                    compound
                        .requires_hover
                        .then_some(crate::engine::invalidation::HOVER_STATE.into()),
                )
                .collect(),
        }
    }
}

/// The uses recorded for `name`, copying the name only when it is new.
pub(super) fn use_of<'a>(uses: &'a mut HashMap<String, ClassUse>, name: &str) -> &'a mut ClassUse {
    if !uses.contains_key(name) {
        uses.insert(name.to_owned(), ClassUse::default());
    }
    uses.get_mut(name).expect("inserted above")
}

/// The uses recorded for `name` in compounds with key `guard`.
pub(super) fn guarded_use<'a>(
    uses: &'a mut HashMap<String, HashMap<SubjectKey, ClassUse>>,
    name: &str,
    guard: &SubjectKey,
) -> &'a mut ClassUse {
    if !uses.contains_key(name) {
        uses.insert(name.to_owned(), HashMap::new());
    }
    let guards = uses.get_mut(name).expect("inserted above");
    if !guards.contains_key(guard) {
        guards.insert(guard.clone(), ClassUse::default());
    }
    guards.get_mut(guard).expect("inserted above")
}

/// Features inside `:is()`, `:where()` and `:not()` arguments that are single compounds describe
/// the same element as `compound`. Any other nesting (complex arguments, `:has()`, `of S`) is
/// collected as unbounded.
pub(super) fn nested_simple_features<'a>(
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
pub(super) fn simple_class_attribute(compound: &CompoundSelector) -> bool {
    is_class_attribute(compound)
        || compound.functional.iter().any(|functional| {
            functional.selectors.iter().any(|selector| {
                selector.compounds.len() == 1 && simple_class_attribute(&selector.compounds[0])
            })
        })
}

/// A `[class]` selector inside a nested complex selector, `:has()` or `of S`.
pub(super) fn nested_complex_class_attribute(compound: &CompoundSelector) -> bool {
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
