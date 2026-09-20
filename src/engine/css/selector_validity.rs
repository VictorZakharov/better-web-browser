//! `:valid`, `:invalid`, `:required`, `:optional`, `:in-range`, and
//! `:out-of-range` matching over authoritative control state.
//!
//! Anchors: HTML §4.16.3 (pseudo-classes). `:valid`/`:invalid` apply to
//! candidates for constraint validation (the same predicate as the IDL
//! `willValidate`), plus `form`/`fieldset` aggregation over failing owned or
//! descendant candidates. Patterns evaluate through the cached verdict only,
//! so matching never re-enters script; scripted value/pattern writes refresh
//! that cache on their own realm (see `forms_validity.js`).

use super::dom::{Node, NodeRef};
use crate::engine::dom::node::control_validity::{PatternSource, validity_of, will_validate};

pub(super) fn matches_valid(node: &NodeRef) -> bool {
    match node.tag_name() {
        Some("form") => !form_has_failing_candidate(node),
        Some("fieldset") => !fieldset_has_failing_candidate(node),
        _ => is_satisfying_candidate(node),
    }
}

pub(super) fn matches_invalid(node: &NodeRef) -> bool {
    match node.tag_name() {
        Some("form") => form_has_failing_candidate(node),
        Some("fieldset") => fieldset_has_failing_candidate(node),
        _ => is_failing_candidate(node),
    }
}

fn is_satisfying_candidate(node: &NodeRef) -> bool {
    will_validate(node) && validity_of(node, &PatternSource::Cached).valid()
}

fn is_failing_candidate(node: &NodeRef) -> bool {
    will_validate(node) && !validity_of(node, &PatternSource::Cached).valid()
}

/// A form matches `:invalid` when it owns a failing candidate, including
/// external `form=` controls; otherwise (even with no controls) `:valid`.
fn form_has_failing_candidate(form: &NodeRef) -> bool {
    let root = Node::tree_root(form);
    let id = form.id();
    Node::descendants(&root).any(|candidate| {
        candidate.form_owner().is_some_and(|owner| owner == id) && is_failing_candidate(&candidate)
    })
}

/// A fieldset aggregates failing candidates among all its descendants.
fn fieldset_has_failing_candidate(fieldset: &NodeRef) -> bool {
    Node::descendants(fieldset).any(|candidate| is_failing_candidate(&candidate))
}

pub(super) fn matches_required(node: &NodeRef) -> bool {
    match node.tag_name() {
        Some("input") => {
            node.attr("required").is_some() && required_applies_to_input(&node.input_state_name())
        }
        Some("select" | "textarea") => node.attr("required").is_some(),
        _ => false,
    }
}

pub(super) fn matches_optional(node: &NodeRef) -> bool {
    match node.tag_name() {
        Some("input") => {
            node.attr("required").is_none() && required_applies_to_input(&node.input_state_name())
        }
        Some("select" | "textarea") => node.attr("required").is_none(),
        _ => false,
    }
}

/// Input states where `required` applies. Hidden and button states match
/// neither `:required` nor `:optional`; readonly/disabled do not change
/// requiredness (only candidacy). Verified against headless Chrome in Phase E.
fn required_applies_to_input(state: &str) -> bool {
    !matches!(state, "hidden" | "submit" | "image" | "reset" | "button")
}

pub(super) fn matches_in_range(node: &NodeRef) -> bool {
    range_flags(node).is_some_and(|(underflow, overflow)| !underflow && !overflow)
}

pub(super) fn matches_out_of_range(node: &NodeRef) -> bool {
    range_flags(node).is_some_and(|(underflow, overflow)| underflow || overflow)
}

/// Underflow/overflow only when the node has applicable range limitations:
/// a number/range candidate with `min` or `max` specified. An empty value
/// parses to nothing, suffers neither bound, and therefore counts as
/// in-range; text inputs never have range limitations.
fn range_flags(node: &NodeRef) -> Option<(bool, bool)> {
    if node.tag_name() != Some("input") {
        return None;
    }
    if !matches!(node.input_state_name().as_str(), "number" | "range") {
        return None;
    }
    if !will_validate(node) {
        return None;
    }
    if node.attr("min").is_none() && node.attr("max").is_none() {
        return None;
    }
    let flags = validity_of(node, &PatternSource::Cached);
    Some((flags.range_underflow, flags.range_overflow))
}

#[cfg(test)]
mod tests {
    use super::super::selector_match::compile_selector_list;
    use super::super::selector_parser::parse_selector;
    use super::*;
    use crate::engine::dom::node::control_validity::store_pattern_verdict;
    use crate::engine::dom::parse;

    /// Asserts per-element match results for `tag` in document order.
    fn assert_match(html: &str, selector: &str, tag: &str, expected: &[bool]) {
        let dom = parse(html);
        let list = compile_selector_list(selector).expect("selector parses");
        let actual: Vec<bool> = dom
            .elements_named(tag)
            .map(|node| list.matches(&node))
            .collect();
        assert_eq!(actual, expected, "{selector} on {html}");
    }

    #[test]
    fn required_text_tracks_value() {
        let html = "<form><input required><input required value=x></form>";
        assert_match(html, ":invalid", "input", &[true, false]);
        assert_match(html, ":valid", "input", &[false, true]);
        assert_match(html, ":required", "input", &[true, true]);
        assert_match(html, ":optional", "input", &[false, false]);
        assert_match(html, "input:required:invalid", "input", &[true, false]);
        assert_match(html, "input:not(:valid)", "input", &[true, false]);
    }

    #[test]
    fn required_applies_by_type_not_bar_status() {
        let html = "<input type=hidden required><input type=submit required>\
            <input><input type=range><select></select><textarea></textarea><div required></div>";
        assert_match(html, ":required", "input", &[false, false, false, false]);
        assert_match(html, ":optional", "input", &[false, false, true, true]);
        assert_match(html, ":required", "select", &[false]);
        assert_match(html, ":optional", "select", &[true]);
        assert_match(html, ":required", "textarea", &[false]);
        assert_match(html, ":optional", "textarea", &[true]);
        assert_match(html, ":required", "div", &[false]);
        assert_match(html, ":optional", "div", &[false]);
        // Barred controls match neither validity state but keep requiredness.
        let barred = "<input required disabled value=><input required readonly value=>";
        assert_match(barred, ":required", "input", &[true, true]);
        assert_match(barred, ":valid", "input", &[false, false]);
        assert_match(barred, ":invalid", "input", &[false, false]);
    }

    #[test]
    fn readonly_checkbox_stays_a_candidate() {
        let html = "<input type=checkbox required readonly>";
        assert_match(html, ":invalid", "input", &[true]);
        assert_match(html, ":required", "input", &[true]);
    }

    #[test]
    fn user_edit_flips_validity_dynamically() {
        let dom = parse("<form><input required></form>");
        let input = dom.elements_named("input").next().expect("input");
        let invalid = compile_selector_list(":invalid").expect("selector parses");
        let valid = compile_selector_list(":valid").expect("selector parses");
        assert!(invalid.matches(&input));
        assert!(input.user_edit_input("a"));
        assert!(valid.matches(&input));
        assert!(!invalid.matches(&input));
        let form = dom.elements_named("form").next().expect("form");
        assert!(valid.matches(&form));
    }

    #[test]
    fn form_and_fieldset_aggregate() {
        let html = "<form id=a><input required></form>\
            <form id=b><input required value=x></form><form id=c></form>\
            <fieldset id=d><input required></fieldset>\
            <fieldset id=e><input required value=x></fieldset><div></div>";
        assert_match(html, ":invalid", "form", &[true, false, false]);
        assert_match(html, ":valid", "form", &[false, true, true]);
        assert_match(html, ":invalid", "fieldset", &[true, false]);
        assert_match(html, ":valid", "fieldset", &[false, true]);
        assert_match(html, ":invalid", "div", &[false]);
        assert_match(html, ":valid", "div", &[false]);
    }

    #[test]
    fn external_form_control_counts_toward_its_owner() {
        let dom = parse("<form id=f></form><input form=f required>");
        let form = dom.elements_named("form").next().expect("form");
        let invalid = compile_selector_list(":invalid").expect("selector parses");
        let valid = compile_selector_list(":valid").expect("selector parses");
        assert!(invalid.matches(&form));
        let input = dom.elements_named("input").next().expect("input");
        assert!(input.user_edit_input("a"));
        assert!(valid.matches(&form));
        assert!(!invalid.matches(&form));
    }

    #[test]
    fn numeric_range_states() {
        let html = "<form>\
            <input type=number min=1 max=10 value=5>\
            <input type=number min=1 max=10 value=0>\
            <input type=number min=1 max=10 value=>\
            <input type=number value=0>\
            <input type=text min=1 value=0>\
            <input type=number min=1 max=10 value=0 disabled>\
            </form>";
        assert_match(
            html,
            ":in-range",
            "input",
            &[true, false, true, false, false, false],
        );
        assert_match(
            html,
            ":out-of-range",
            "input",
            &[false, true, false, false, false, false],
        );
        // Empty parses to nothing: no bound suffered, so in-range and valid.
        assert_match(
            html,
            ":valid",
            "input",
            &[true, false, true, true, true, false],
        );
    }

    #[test]
    fn checked_option_follows_selectedness_not_attributes() {
        let dom = parse("<select><option value=a>A</option><option value=b>B</option></select>");
        let select = dom.elements_named("select").next().expect("select");
        let options: Vec<NodeRef> = dom.elements_named("option").collect();
        assert!(select.user_pick_option("b"));
        let checked = compile_selector_list(":checked").expect("selector parses");
        assert!(!checked.matches(&options[0]));
        assert!(checked.matches(&options[1]));
        let dom =
            parse("<select><option value=a selected>A</option><option value=b>B</option></select>");
        let options: Vec<NodeRef> = dom.elements_named("option").collect();
        assert!(checked.matches(&options[0]));
        assert!(!checked.matches(&options[1]));
    }

    #[test]
    fn cached_pattern_verdict_drives_selectors_without_script() {
        let dom = parse("<input pattern='a+' value='bbb'>");
        let input = dom.elements_named("input").next().expect("input");
        let invalid = compile_selector_list(":invalid").expect("selector parses");
        let valid = compile_selector_list(":valid").expect("selector parses");
        // Cold cache means "no known mismatch": selectors never run script.
        assert!(valid.matches(&input));
        assert!(store_pattern_verdict(
            &input,
            "a+",
            vec!["bbb".to_string()],
            false,
        ));
        assert!(invalid.matches(&input));
        assert!(!valid.matches(&input));
        // A stale verdict (different values) does not apply.
        assert!(input.user_edit_input("aaa"));
        assert!(valid.matches(&input));
    }

    #[test]
    fn required_radio_group_validity_is_per_group() {
        let dom = parse("<form><input type=radio name=g required><input type=radio name=g></form>");
        let radios: Vec<NodeRef> = dom.elements_named("input").collect();
        let invalid = compile_selector_list(":invalid").expect("selector parses");
        let valid = compile_selector_list(":valid").expect("selector parses");
        assert!(invalid.matches(&radios[0]));
        // The unrequired peer has no failing constraint of its own.
        assert!(valid.matches(&radios[1]));
        let form = dom.elements_named("form").next().expect("form");
        assert!(invalid.matches(&form));
        radios[1].set_checked(true, true);
        assert!(valid.matches(&radios[0]));
        assert!(valid.matches(&radios[1]));
        assert!(valid.matches(&form));
    }

    #[test]
    fn required_attribute_change_applies_dynamically() {
        let dom = parse("<input>");
        let input = dom.elements_named("input").next().expect("input");
        let required = compile_selector_list(":required").expect("selector parses");
        let optional = compile_selector_list(":optional").expect("selector parses");
        assert!(optional.matches(&input));
        assert!(input.set_attr("required", ""));
        assert!(required.matches(&input));
        assert!(!optional.matches(&input));
    }

    #[test]
    fn validation_pseudos_count_as_a_class_of_specificity() {
        let selector = parse_selector("input:invalid").expect("selector parses");
        assert_eq!(selector.specificity.ids, 0);
        assert_eq!(selector.specificity.classes, 1);
        assert_eq!(selector.specificity.tags, 1);
    }

    #[test]
    fn single_edit_refresh_is_proportional_not_document_wide() {
        use super::super::StyleSet;
        use crate::engine::invalidation::validation_aggregation_roots;
        // Five forms of 100 validated inputs plus unrelated content.
        let mut html = String::from(
            "<style>input:invalid{color:red}input:valid{color:green}form:invalid{color:red}</style>",
        );
        for form in 0..5 {
            html.push_str(&format!("<form id=f{form}>"));
            for index in 0..100 {
                if index % 2 == 0 {
                    html.push_str(&format!(
                        "<input name=v{form}-{index} required value=seed{index}>"
                    ));
                } else {
                    html.push_str(&format!("<input name=v{form}-{index} required>"));
                }
            }
            html.push_str("</form>");
        }
        html.push_str("<p>Unrelated content that must not restyle on control edits.</p>");
        let dom = parse(&html);
        let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
        let edited = dom.elements_named("input").nth(1).expect("second input");
        assert!(edited.user_edit_input("fixed"));
        // Roots exactly as the mutation machinery records them: the control's
        // subtree root plus form/fieldset aggregates.
        let mut roots = vec![
            edited
                .shadow_including_parent()
                .unwrap_or_else(|| edited.clone()),
        ];
        roots.extend(validation_aggregation_roots(&edited));
        let stats = styles.refresh_subtrees(&dom.document, &roots, &[]);
        let total: usize = dom.elements_named("input").count();
        assert_eq!(total, 500);
        // One form subtree (~100 inputs plus its form node), not the document.
        assert!(
            stats.recomputed_styles < 250,
            "recomputed {} styles for one edit",
            stats.recomputed_styles
        );
        // The bound is meaningful: a whole-document refresh costs the full set.
        let mut fresh = StyleSet::from_dom(&dom, &[], 800.0);
        let whole = fresh.refresh_subtrees(&dom.document, std::slice::from_ref(&dom.document), &[]);
        assert!(whole.recomputed_styles >= 500, "{whole:?}");
        assert!(stats.recomputed_styles < whole.recomputed_styles);
        // And the refresh actually applied the new state.
        let valid = compile_selector_list(":valid").expect("selector parses");
        assert!(valid.matches(&edited));
    }
}
