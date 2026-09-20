//! Single constraint-validation algorithm for script, selectors, and native
//! submission. Validity is computed on demand from authoritative control
//! state; nothing caches verdicts except short-lived query snapshots.
//!
//! Anchors: HTML §4.10.21 (validation), §4.10.5 (input states), §4.10.7/10/11
//! (select/option/textarea). Pattern matching goes through the V8 boundary in
//! [`crate::engine::pattern_eval`]; every other flag is dependency-free.

use super::NodeRef;
use super::control_numeric::{allowed_step, step_base, step_mismatch};
use super::control_state::ControlState;
use super::control_values::parse_float_value;
use super::control_values::parse_non_negative;
use crate::engine::css::selector_match::is_disabled;

/// All validity flags; `valid` is the absence of every other flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValidityFlags {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: bool,
}

impl ValidityFlags {
    pub(crate) fn valid(&self) -> bool {
        !(self.value_missing
            || self.type_mismatch
            || self.pattern_mismatch
            || self.too_long
            || self.too_short
            || self.range_underflow
            || self.range_overflow
            || self.step_mismatch
            || self.bad_input
            || self.custom_error)
    }

    pub(crate) fn to_json(self) -> String {
        serde_json::json!({
            "valueMissing": self.value_missing,
            "typeMismatch": self.type_mismatch,
            "patternMismatch": self.pattern_mismatch,
            "tooLong": self.too_long,
            "tooShort": self.too_short,
            "rangeUnderflow": self.range_underflow,
            "rangeOverflow": self.range_overflow,
            "stepMismatch": self.step_mismatch,
            "badInput": self.bad_input,
            "customError": self.custom_error,
            "valid": self.valid(),
        })
        .to_string()
    }
}

/// Whether the element is a candidate for constraint validation.
pub(crate) fn will_validate(node: &NodeRef) -> bool {
    if has_datalist_ancestor(node) {
        return false;
    }
    match node.tag_name() {
        Some("input") => {
            let state = node.input_state_name();
            if matches!(state.as_str(), "hidden" | "reset" | "button") {
                return false;
            }
            if is_disabled(node) {
                return false;
            }
            // Readonly bars validation only where it applies; elsewhere the
            // attribute is ignored rather than disabling the control.
            if node.attr("readonly").is_some() && readonly_applies(&state) {
                return false;
            }
            true
        }
        Some("textarea") => {
            if is_disabled(node) {
                return false;
            }
            node.attr("readonly").is_none()
        }
        Some("select") => !is_disabled(node),
        Some("button") => is_submit_button(node) && !is_disabled(node),
        _ => false,
    }
}

/// Readonly applies to text-like, password, temporal, and number states.
fn readonly_applies(state: &str) -> bool {
    matches!(
        state,
        "text"
            | "search"
            | "tel"
            | "url"
            | "email"
            | "password"
            | "date"
            | "month"
            | "week"
            | "time"
            | "datetime-local"
            | "number"
    )
}

/// Submit buttons are validation candidates; reset/button are barred.
pub(crate) fn is_submit_button(node: &NodeRef) -> bool {
    if node.tag_name() != Some("button") {
        return false;
    }
    let state = node.attr("type").unwrap_or_default().to_ascii_lowercase();
    !matches!(state.as_str(), "reset" | "button")
}

fn has_datalist_ancestor(node: &NodeRef) -> bool {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if current.tag_name() == Some("datalist") {
            return true;
        }
        ancestor = current.parent();
    }
    false
}

/// Where pattern verdicts come from. `Live` evaluates the regex now;
/// `Cached` reuses the verdict stored by the last scripted evaluation whose
/// (pattern, values) still match, so selector matching never re-enters V8.
pub(crate) enum PatternSource<'a> {
    Live(&'a dyn Fn(&str, &str) -> Option<bool>),
    Cached,
}

/// Computes every validity flag for a candidate; barred elements are valid.
pub(crate) fn validity_of(node: &NodeRef, patterns: &PatternSource) -> ValidityFlags {
    let mut flags = ValidityFlags::default();
    if !will_validate(node) {
        return flags;
    }
    let snapshot = node.control_state_snapshot();
    flags.custom_error = !snapshot.custom_message.is_empty();
    match node.tag_name() {
        Some("input") => validity_for_input(node, &snapshot, &mut flags, patterns),
        Some("textarea") => {
            let value = node.textarea_api_value();
            flags.value_missing = node.attr("required").is_some() && value.is_empty();
            length_flags(node, &value, &snapshot, &mut flags);
        }
        Some("select") => {
            flags.value_missing = node.attr("required").is_some() && select_is_missing(node);
        }
        Some("button") => {}
        _ => {}
    }
    flags
}

fn validity_for_input(
    node: &NodeRef,
    snapshot: &ControlState,
    flags: &mut ValidityFlags,
    patterns: &PatternSource,
) {
    let state = node.input_state_name();
    let required = node.attr("required").is_some();
    let value = node.input_value();
    match state.as_str() {
        "checkbox" => {
            flags.value_missing = required && !node.checked();
        }
        "radio" => {
            let group = node.radio_group();
            flags.value_missing = required
                && if group.is_empty() {
                    !node.checked()
                } else {
                    group.iter().all(|peer| !peer.checked())
                };
        }
        "file" => {
            // No picker exists; required files are always missing.
            flags.value_missing = required;
        }
        "range" | "submit" | "image" | "reset" | "button" | "hidden" => {}
        _ if required
            && value.is_empty()
            && (super::control_values::text_like_type(&state)
                || state == "password"
                || state == "number"
                || is_temporal_state(&state)) =>
        {
            flags.value_missing = true;
        }
        _ => {}
    }
    if state == "url" && !value.is_empty() && url::Url::parse(&value).is_err() {
        flags.type_mismatch = true;
    }
    if state == "email" && !value.is_empty() {
        let multiple = node.attr("multiple").is_some();
        flags.type_mismatch = !email_matches(&value, multiple);
    }
    if matches!(
        state.as_str(),
        "text" | "search" | "tel" | "url" | "email" | "password"
    ) && !value.is_empty()
        && let Some((pattern, values)) = current_pattern_inputs(node, &state, &value)
    {
        flags.pattern_mismatch = match patterns {
            PatternSource::Live(test) => values
                .iter()
                .any(|candidate| test(&pattern, candidate).is_some_and(|matched| !matched)),
            PatternSource::Cached => {
                cached_pattern_verdict(node, &pattern, &values).is_some_and(|matched| !matched)
            }
        };
    }
    if matches!(
        state.as_str(),
        "text" | "search" | "tel" | "url" | "email" | "password"
    ) {
        length_flags(node, &value, snapshot, flags);
    }
    if matches!(state.as_str(), "number" | "range") {
        numeric_flags(node, &value, snapshot, flags);
    }
}

fn is_temporal_state(state: &str) -> bool {
    matches!(state, "date" | "month" | "week" | "time" | "datetime-local")
}

/// Length flags need dirty state whose last change was a user edit.
fn length_flags(node: &NodeRef, value: &str, snapshot: &ControlState, flags: &mut ValidityFlags) {
    if !snapshot.dirty || !snapshot.user_edited {
        return;
    }
    let length = value.encode_utf16().count();
    if let Some(maximum) = node
        .attr("maxlength")
        .and_then(|limit| parse_non_negative(&limit))
        && length > maximum as usize
    {
        flags.too_long = true;
    }
    if !value.is_empty()
        && let Some(minimum) = node
            .attr("minlength")
            .and_then(|limit| parse_non_negative(&limit))
        && length < minimum as usize
    {
        flags.too_short = true;
    }
}

fn numeric_flags(node: &NodeRef, value: &str, snapshot: &ControlState, flags: &mut ValidityFlags) {
    let state = node.input_state_name();
    if state == "number" && snapshot.editing.is_some() {
        flags.bad_input = true;
    }
    let minimum = node.attr("min").as_deref().and_then(parse_float_value);
    let maximum = node.attr("max").as_deref().and_then(parse_float_value);
    if let Some(actual) = parse_float_value(value) {
        // Number/range are non-periodic: they never have a reversed range,
        // so plain comparisons apply even when max < min (in which case a
        // value suffers whichever bound it violates, possibly both).
        flags.range_underflow = minimum.is_some_and(|minimum| actual < minimum);
        flags.range_overflow = maximum.is_some_and(|maximum| actual > maximum);
        if let Some(step) = allowed_step(&state, &node.attr("step")) {
            let base = step_base(&node.attr("min"), &node.attr("value"));
            flags.step_mismatch = step_mismatch(value, base, step);
        }
    }
}

/// Required select is missing with no selection or only the placeholder.
fn select_is_missing(select: &NodeRef) -> bool {
    let options = select.select_options();
    let selected: Vec<&NodeRef> = options
        .iter()
        .filter(|option| option.control_state_snapshot().selectedness)
        .collect();
    if selected.is_empty() {
        return true;
    }
    if selected.len() == 1
        && let Some(placeholder) = select.placeholder_option()
    {
        return selected[0].id() == placeholder.id();
    }
    false
}

/// Pattern source plus the exact values to test (multiple emails split).
/// Shared by live evaluators so applicability stays in one place.
pub(crate) fn current_pattern_inputs(
    node: &NodeRef,
    state: &str,
    value: &str,
) -> Option<(String, Vec<String>)> {
    let pattern = node.attr("pattern")?;
    let values: Vec<String> = if state == "email" && node.attr("multiple").is_some() {
        value
            .split(',')
            .map(|part| part.trim().to_string())
            .collect()
    } else {
        vec![value.to_string()]
    };
    Some((pattern, values))
}

/// Cached verdict when the stored (pattern, values) still match.
/// See `store_pattern_verdict` for the writer side.
pub(crate) fn cached_pattern_verdict(
    node: &NodeRef,
    pattern: &str,
    values: &[String],
) -> Option<bool> {
    let snapshot = node.control_state_snapshot();
    let (cached_pattern, cached_values, verdict) = snapshot.pattern_verdict.as_ref()?;
    if cached_pattern == pattern && cached_values == values {
        return Some(*verdict);
    }
    None
}

/// Stores a scripted pattern verdict; returns true when it changed something
/// selectors may depend on (callers record a state invalidation then).
pub(crate) fn store_pattern_verdict(
    node: &NodeRef,
    pattern: &str,
    values: Vec<String>,
    verdict: bool,
) -> bool {
    let snapshot = node.control_state_snapshot();
    let changed = snapshot.pattern_verdict.as_ref().is_none_or(
        |(cached_pattern, cached_values, cached_verdict)| {
            cached_pattern != pattern || cached_values != &values || *cached_verdict != verdict
        },
    );
    node.update_control_state_tracked(|state| {
        state.pattern_verdict = Some((pattern.to_string(), values, verdict));
    });
    changed
}
/// Single email address per the HTML grammar (single-label domains valid).
fn is_valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if !local.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'.'
                | b'!'
                | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
                | b'-'
        )
    }) {
        return false;
    }
    for label in domain.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 63
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
            || !bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return false;
        }
    }
    true
}

fn email_matches(value: &str, multiple: bool) -> bool {
    if !multiple {
        return is_valid_email(value);
    }
    value.split(',').map(str::trim).all(is_valid_email)
}

/// Actionable, category-distinguishing message; empty when valid or barred.
pub(crate) fn validation_message(node: &NodeRef, flags: &ValidityFlags) -> String {
    if !will_validate(node) || flags.valid() {
        return String::new();
    }
    if flags.custom_error {
        return node.control_state_snapshot().custom_message.clone();
    }
    if flags.value_missing {
        return "Please fill out this field.".to_string();
    }
    if flags.type_mismatch {
        return match node.tag_name() {
            Some("input") if node.input_state_name() == "email" => {
                "Please enter an email address.".to_string()
            }
            Some("input") if node.input_state_name() == "url" => "Please enter a URL.".to_string(),
            _ => "Please match the requested format.".to_string(),
        };
    }
    if flags.pattern_mismatch {
        return "Please match the requested format.".to_string();
    }
    if flags.too_long {
        return "Please shorten this text.".to_string();
    }
    if flags.too_short {
        return "Please lengthen this text.".to_string();
    }
    if flags.range_underflow {
        return "Value is below the minimum.".to_string();
    }
    if flags.range_overflow {
        return "Value is above the maximum.".to_string();
    }
    if flags.step_mismatch {
        return "Please enter a valid stepped value.".to_string();
    }
    if flags.bad_input {
        return "Please enter a number.".to_string();
    }
    "Please enter a valid value.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::parse;

    #[test]
    fn custom_message_round_trips_through_validity() {
        let dom = parse("<body><input required></body>");
        let input = dom.elements_named("input").next().expect("input");
        input.set_custom_message("Custom problem.");
        let flags = validity_of(
            &input,
            &PatternSource::Live(&crate::engine::pattern_eval::test_pattern),
        );
        assert!(flags.custom_error);
        assert!(flags.value_missing);
        assert!(!flags.valid());
        assert_eq!(validation_message(&input, &flags), "Custom problem.");
    }

    #[test]
    fn email_grammar_accepts_single_label_domains() {
        for valid in ["user@intranet", "a@b", "timbl@w3.org", "x.y+z@sub.domain"] {
            assert!(is_valid_email(valid), "{valid}");
        }
        for invalid in [
            "", "plain", "@b", "a@", "a@b@c", "a@-b", "a@b-", "a b@c", "ä@b",
        ] {
            assert!(!is_valid_email(invalid), "{invalid}");
        }
        assert!(email_matches("a@b, c@d", true));
        assert!(!email_matches("a@b, nope", true));
    }
}
