//! Internal selector representation and specificity ordering.

use std::cmp::Ordering;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub(super) struct Selector {
    // Sharing the immutable selector graph keeps nested selector lists linear in nesting depth.
    pub(super) compounds: Rc<Vec<CompoundSelector>>,
    pub(super) combinators: Rc<Vec<Combinator>>,
    pub(super) specificity: Specificity,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CompoundSelector {
    pub(super) tag: Option<String>,
    pub(super) id: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) attributes: Vec<AttributeSelector>,
    pub(super) functional: Vec<FunctionalSelector>,
    pub(super) has: Vec<Vec<RelativeSelector>>,
    pub(super) nth: Vec<NthSelector>,
    pub(super) languages: Vec<Vec<String>>,
    pub(super) directions: Vec<TextDirection>,
    pub(super) requires_link: bool,
    pub(super) requires_first_child: bool,
    pub(super) requires_first_of_type: bool,
    pub(super) requires_last_child: bool,
    pub(super) requires_last_of_type: bool,
    pub(super) requires_only_child: bool,
    pub(super) requires_only_of_type: bool,
    pub(super) requires_empty: bool,
    pub(super) requires_root: bool,
    pub(super) requires_scope: bool,
    pub(super) requires_enabled: bool,
    pub(super) requires_disabled: bool,
    pub(super) requires_read_write: bool,
    pub(super) requires_read_only: bool,
    pub(super) requires_fullscreen: bool,
    pub(super) requires_hover: bool,
    pub(super) requires_focus: bool,
    pub(super) requires_focus_within: bool,
    pub(super) requires_checked: bool,
    pub(super) requires_indeterminate: bool,
    pub(super) requires_valid: bool,
    pub(super) requires_invalid: bool,
    pub(super) requires_required: bool,
    pub(super) requires_optional: bool,
    pub(super) requires_in_range: bool,
    pub(super) requires_out_of_range: bool,
    pub(super) never_matches: bool,
}

#[derive(Debug, Clone)]
pub(super) struct AttributeSelector {
    pub(super) name: String,
    pub(super) operator: AttributeOperator,
    pub(super) value: String,
    pub(super) case_sensitivity: AttributeCaseSensitivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttributeCaseSensitivity {
    Default,
    AsciiInsensitive,
    Sensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextDirection {
    Ltr,
    Rtl,
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
pub(super) struct FunctionalSelector {
    pub(super) kind: FunctionalSelectorKind,
    pub(super) selectors: Vec<Selector>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FunctionalSelectorKind {
    Is,
    Where,
    Not,
}

#[derive(Debug, Clone)]
pub(super) struct RelativeSelector {
    pub(super) selector: Selector,
    pub(super) search: RelativeSearch,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RelativeSearch {
    Descendants,
    Children,
    FollowingSiblings,
    NextSibling,
    LaterSiblings,
}

#[derive(Debug, Clone)]
pub(super) struct NthSelector {
    pub(super) a: i32,
    pub(super) b: i32,
    pub(super) from_end: bool,
    pub(super) of_type: bool,
    pub(super) filter: Vec<Selector>,
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
