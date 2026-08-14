//! Internal selector representation and specificity ordering.

use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub(super) struct Selector {
    pub(super) compounds: Vec<CompoundSelector>,
    pub(super) combinators: Vec<Combinator>,
    pub(super) specificity: Specificity,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CompoundSelector {
    pub(super) tag: Option<String>,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) attributes: Vec<AttributeSelector>,
    pub(super) any_of: Vec<Vec<SimpleSelector>>,
    pub(super) not: Vec<Vec<SimpleSelector>>,
    pub(super) requires_link: bool,
    pub(super) requires_first_child: bool,
    pub(super) requires_root: bool,
    pub(super) requires_enabled: bool,
    pub(super) requires_disabled: bool,
    pub(super) never_matches: bool,
}

#[derive(Debug, Clone)]
pub(super) struct AttributeSelector {
    pub(super) name: String,
    pub(super) operator: AttributeOperator,
    pub(super) value: String,
    pub(super) case_insensitive: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AttributeOperator {
    Exists,
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone)]
pub(super) enum SimpleSelector {
    Tag(String),
    Id(String),
    Class(String),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Specificity {
    pub(super) ids: u16,
    pub(super) classes: u16,
    pub(super) tags: u16,
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.ids, self.classes, self.tags).cmp(&(other.ids, other.classes, other.tags))
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
