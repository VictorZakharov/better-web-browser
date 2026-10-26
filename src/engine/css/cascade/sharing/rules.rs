//! Which rules let elements share styles, and which ids any selector names.
use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuleSharing {
    /// The result follows from inputs the cache compares: the element's and its ancestors'
    /// names, classes, ids and compared attribute values.
    KeyDetermined,
    /// Evaluate on both elements; equal results keep the matched rule lists equal.
    Revalidate,
    /// Scoped, shadow and part rules order or apply by inputs the cache does not compare.
    Unshareable,
}

pub(super) fn referenced_ids(rules: &[Rule]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for rule in rules {
        let scopes = rule
            .css_scopes
            .iter()
            .flat_map(|scope| scope.start.iter().chain(scope.end.iter()).flatten());
        let selectors = std::iter::once(&rule.selector)
            .chain(rule.host_condition.iter())
            .chain(rule.slotted_origin.iter())
            .chain(scopes);
        for selector in selectors {
            collect_ids(selector, &mut ids);
        }
    }
    ids
}

fn collect_ids(selector: &Selector, ids: &mut HashSet<String>) {
    for compound in selector.compounds.iter() {
        ids.extend(compound.id.iter().map(|id| id.to_ascii_lowercase()));
        let nested = compound
            .functional
            .iter()
            .flat_map(|functional| functional.selectors.iter())
            .chain(
                compound
                    .has
                    .iter()
                    .flatten()
                    .map(|relative| &relative.selector),
            )
            .chain(compound.nth.iter().flat_map(|nth| nth.filter.iter()));
        for selector in nested {
            collect_ids(selector, ids);
        }
    }
}

pub(super) fn classify_rules(rules: &[Rule]) -> Vec<RuleSharing> {
    rules.iter().map(classify_rule).collect()
}

fn classify_rule(rule: &Rule) -> RuleSharing {
    if rule.part.is_some()
        || rule.host_condition.is_some()
        || rule.slotted_origin.is_some()
        || !rule.css_scopes.is_empty()
        || !matches!(rule.scope, RuleScope::Document)
    {
        return RuleSharing::Unshareable;
    }
    let combinators_key_determined = rule
        .selector
        .combinators
        .iter()
        .all(|combinator| matches!(combinator, Combinator::Descendant | Combinator::Child));
    if combinators_key_determined && rule.selector.compounds.iter().all(key_determined) {
        RuleSharing::KeyDetermined
    } else {
        RuleSharing::Revalidate
    }
}

/// Exhaustive so that a new selector feature is classified before it compiles.
fn key_determined(compound: &CompoundSelector) -> bool {
    let CompoundSelector {
        tag: _,
        id: _,
        classes: _,
        never_matches: _,
        attributes,
        functional,
        has,
        nth,
        languages: _,
        directions,
        requires_link,
        requires_unvisited,
        requires_visited,
        requires_first_child,
        requires_first_of_type,
        requires_last_child,
        requires_last_of_type,
        requires_only_child,
        requires_only_of_type,
        requires_empty,
        requires_has_slotted,
        requires_root: _,
        requires_scope,
        requires_enabled,
        requires_disabled,
        requires_read_write,
        requires_read_only,
        requires_fullscreen,
        requires_popover_open,
        requires_hover,
        requires_focus,
        requires_focus_within,
        requires_checked,
        requires_indeterminate,
        requires_valid,
        requires_invalid,
        requires_required,
        requires_optional,
        requires_in_range,
        requires_out_of_range,
    } = compound;
    let flags = [
        requires_link,
        requires_unvisited,
        requires_visited,
        requires_first_child,
        requires_first_of_type,
        requires_last_child,
        requires_last_of_type,
        requires_only_child,
        requires_only_of_type,
        requires_empty,
        requires_has_slotted,
        requires_scope,
        requires_enabled,
        requires_disabled,
        requires_read_write,
        requires_read_only,
        requires_fullscreen,
        requires_popover_open,
        requires_hover,
        requires_focus,
        requires_focus_within,
        requires_checked,
        requires_indeterminate,
        requires_valid,
        requires_invalid,
        requires_required,
        requires_optional,
        requires_in_range,
        requires_out_of_range,
    ];
    // Paired elements and ancestors have equal names, attribute lists and values except
    // for selector-only attributes, so presence tests and tests of compared values agree.
    // `:lang()` reads compared `lang` values up the paired ancestor chains, and `:root`
    // matches neither paired element (the root element never shares).
    attributes.iter().all(|attribute| {
        matches!(attribute.operator, AttributeOperator::Exists)
            || !super::inputs::selector_only_attribute(&attribute.name.to_ascii_lowercase())
    }) && functional.iter().all(|functional| {
        // A selector list on the same element, without combinators, is decided by the
        // same inputs; `:not()` only negates that result.
        functional.selectors.iter().all(|selector| {
            selector.combinators.is_empty() && selector.compounds.iter().all(key_determined)
        })
    }) && has.is_empty()
        && nth.is_empty()
        && directions.is_empty()
        && flags.iter().all(|flag| !**flag)
}
