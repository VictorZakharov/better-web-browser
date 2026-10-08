//! Test deterministic resource limits instead of fragile wall-clock deadlines.
use super::*;

fn doubled(depth: usize, seed: &str) -> HashMap<String, String> {
    let mut values = HashMap::from([("--v0".into(), seed.into())]);
    for index in 1..=depth {
        values.insert(
            format!("--v{index}"),
            format!("var(--v{}) var(--v{})", index - 1, index - 1),
        );
    }
    values
}

#[test]
fn small_repeated_expansions_preserve_all_tokens() {
    let values = doubled(8, "token");
    let value = substitute_variable_references("var(--v8)", &values, &mut Vec::new(), 0).unwrap();
    assert_eq!(value.split_whitespace().count(), 256);
    assert!(value.split_whitespace().all(|token| token == "token"));
}

#[test]
fn cumulative_source_work_is_bounded_even_for_one_token_per_large_input() {
    let text = "x".repeat(256 * 1024);
    let mut budget = Budget::default();
    for _ in 0..16 {
        assert_eq!(budget.source(&text, 0), Some(()));
        assert_eq!(budget.step(), Some(()));
    }
    assert_eq!(budget.source(&text, 0), None);
    assert!(budget.exhausted);
}

#[test]
fn comment_heavy_references_cannot_hide_exponential_parsing_work() {
    let comment = format!("/*{}*/", "x".repeat(128 * 1024));
    let values = doubled(8, &comment);
    let mut budget = Budget::default();
    let mut stack = Vec::new();
    assert_eq!(
        references("var(--v8, .5)", &values, &mut stack, 0, &mut budget),
        None
    );
    assert!(budget.exhausted);
    assert!(stack.is_empty());
    // Exhaustion is sticky within a declaration, but never leaks to the next.
    assert_eq!(references(".7", &values, &mut stack, 0, &mut budget), None);
    assert_eq!(
        substitute_variable_references(".7", &values, &mut stack, 0),
        Some(".7".into())
    );
}

#[test]
fn intermediate_serialization_has_a_cumulative_byte_budget() {
    let text = "x".repeat(256 * 1024);
    let mut budget = Budget::default();
    let mut output = String::new();
    for _ in 0..64 {
        output.clear();
        assert_eq!(budget.push(&mut output, &text), Some(()));
        assert_eq!(output, text);
    }
    output.clear();
    assert_eq!(budget.push(&mut output, &text), None);
    assert!(budget.exhausted);
    assert!(output.is_empty(), "the over-budget append must be atomic");
}

#[test]
fn comment_work_exhaustion_invalidates_the_actual_cascade_winner() {
    let comment = format!("/*{}*/", "x".repeat(128 * 1024));
    let declarations = doubled(8, &comment)
        .into_iter()
        .map(|(name, value)| format!("{name}:{value};"))
        .collect::<String>();
    let dom = dom::parse(&format!(
        "<div style='{declarations}opacity:.7;opacity:var(--v8,.5)'></div>"
    ));
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(
        styles
            .get(&dom.elements_named("div").next().unwrap())
            .opacity,
        1.0
    );
}

#[test]
fn exponential_token_growth_is_bounded_even_before_the_depth_limit() {
    let values = doubled(30, "token");
    let mut budget = Budget::default();
    let mut stack = Vec::new();
    assert_eq!(
        references("var(--v30)", &values, &mut stack, 0, &mut budget),
        None
    );
    assert!(budget.exhausted);
    assert!(
        stack.is_empty(),
        "failed branches must unwind their cycle stack"
    );
}

#[test]
fn zero_byte_exponential_growth_still_has_a_work_limit() {
    let values = doubled(30, "")
        .into_iter()
        .map(|(name, value)| (name, value.replace(' ', "")))
        .collect();
    let mut budget = Budget::default();
    assert_eq!(
        references("var(--v30)", &values, &mut Vec::new(), 0, &mut budget),
        None
    );
    assert!(budget.exhausted);
}

#[test]
fn resource_failure_cannot_be_hidden_by_a_fallback() {
    let values = doubled(30, "token");
    assert_eq!(
        substitute_variable_references("var(--v30, .5)", &values, &mut Vec::new(), 0),
        None
    );
    assert_eq!(
        substitute_variable_references("var(--missing, .5)", &values, &mut Vec::new(), 0)
            .as_deref(),
        Some(" .5")
    );
    let mut nested = values;
    nested.insert("--outer".into(), "var(--v30, .5)".into());
    assert_eq!(
        substitute_variable_references("var(--outer, .7)", &nested, &mut Vec::new(), 0),
        None
    );
}

#[test]
fn sizeable_single_tokens_are_not_confused_with_many_expansion_steps() {
    let text = format!("\"{}\"", "x".repeat(128 * 1024));
    let values = HashMap::from([("--large".into(), text.clone())]);
    assert_eq!(
        substitute_variable_references("var(--large)", &values, &mut Vec::new(), 0),
        Some(text)
    );
}

#[test]
fn byte_limit_counts_utf8_and_rejects_oversized_sources_before_copying() {
    let text = format!("\"{}\"", "é".repeat(budget::MAX_BYTES / 2));
    let values = HashMap::from([("--large".into(), text)]);
    let mut budget = Budget::default();
    assert_eq!(
        references("var(--large, .5)", &values, &mut Vec::new(), 0, &mut budget),
        None
    );
    assert!(budget.exhausted);
}

#[test]
fn failed_serialization_never_appends_past_the_byte_limit() {
    let mut output = "x".repeat(budget::MAX_BYTES - 2);
    let mut budget = Budget::default();
    let token = Token::QuotedString("escaped\\quote\"".into());
    assert_eq!(budget.token(&mut output, &token), None);
    assert!(budget.exhausted);
    assert!(output.len() <= budget::MAX_BYTES);
}

#[test]
fn concatenating_two_valid_values_checks_the_combined_result() {
    let text = format!("\"{}\"", "x".repeat(600 * 1024));
    let values = HashMap::from([("--large".into(), text)]);
    assert_eq!(
        substitute_variable_references("var(--large) var(--large)", &values, &mut Vec::new(), 0),
        None
    );
}

#[test]
fn budget_state_is_local_to_one_declaration() {
    let values = doubled(30, "token");
    assert!(substitute_variable_references("var(--v30)", &values, &mut Vec::new(), 0).is_none());
    assert_eq!(
        substitute_variable_references("var(--v0)", &values, &mut Vec::new(), 0).as_deref(),
        Some("token")
    );
}

#[test]
fn excessive_variable_expansion_unsets_the_winner_instead_of_using_earlier_value() {
    let declarations = doubled(30, "token")
        .into_iter()
        .map(|(name, value)| format!("{name}:{value};"))
        .collect::<String>();
    let dom = dom::parse(&format!(
        "<div style='{declarations}opacity:.7;opacity:var(--v30,.5)'></div>"
    ));
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(
        styles
            .get(&dom.elements_named("div").next().unwrap())
            .opacity,
        1.0
    );
}

#[test]
fn numeric_spelling_survives_literal_preparation_and_variable_substitution() {
    for (value, expected) in [
        ("16777217.499", 16_777_217),
        ("16777217.5", 16_777_218),
        ("-16777217.5", -16_777_217),
    ] {
        let custom = HashMap::from([("--n".into(), value.into())]);
        assert_eq!(
            substitute_variable_references("var(--n)", &custom, &mut Vec::new(), 0).as_deref(),
            Some(value)
        );
        for declaration in [
            format!("z-index:calc({value})"),
            format!("--n:{value};z-index:calc(var(--n))"),
        ] {
            let dom = dom::parse(&format!("<div style='{declaration}'></div>"));
            let styles = StyleSet::from_dom(&dom, &[], 800.0);
            assert_eq!(
                styles
                    .get(&dom.elements_named("div").next().unwrap())
                    .z_index,
                Some(expected),
                "{declaration}"
            );
        }
    }
}
