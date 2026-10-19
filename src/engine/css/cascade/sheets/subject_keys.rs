//! Cheap pre-filters for the elements a rule's subject compound can match.
//
// Tag, ID and class requirements of a subject compound bound which elements an invalidation can
// reach. Matching is ASCII case-insensitive, a superset of both standards and quirks matching.
use super::*;
use crate::engine::css::selector_model::CompoundSelector;
use std::collections::{HashMap, HashSet};

/// Tag, ID and classes of a rule's subject compound. An empty key matches any element.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub(super) struct SubjectKey {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

/// Subject keys bucketed by one required feature, so a descendant scan checks only the keys
/// that share a class, ID or tag with each element.
pub(super) struct KeyIndex<'a> {
    by_class: HashMap<String, Vec<&'a SubjectKey>>,
    by_id: HashMap<String, Vec<&'a SubjectKey>>,
    by_tag: HashMap<String, Vec<&'a SubjectKey>>,
}

impl<'a> KeyIndex<'a> {
    pub(super) fn new(keys: HashSet<&'a SubjectKey>) -> Self {
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

    pub(super) fn may_match(&self, node: &NodeRef) -> bool {
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
    pub(super) fn of(compound: &CompoundSelector) -> Self {
        Self {
            tag: compound.tag.clone().filter(|tag| tag != "*"),
            id: compound.id.clone(),
            classes: compound.classes.clone(),
        }
    }

    pub(super) fn matches_any(&self) -> bool {
        self.tag.is_none() && self.id.is_none() && self.classes.is_empty()
    }

    // ASCII case-insensitive comparison is a superset of standards and quirks matching.
    pub(super) fn may_match(&self, node: &NodeRef) -> bool {
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
