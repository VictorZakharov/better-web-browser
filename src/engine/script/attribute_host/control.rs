//! Authoritative form-control state behind the scripted IDL attributes.
//! JS wrappers delegate here; no control value lives in a JS expando.

use super::*;
use crate::engine::dom::Node;
use crate::engine::dom::node::{control_numeric, control_validity, control_values};

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if !matches!(
        operation,
        "inputValue"
            | "inputSetValue"
            | "inputUserEdit"
            | "inputDefaultValue"
            | "textareaValue"
            | "textareaSetValue"
            | "textareaUserEdit"
            | "textareaDefaultValue"
            | "selectSelectedIndex"
            | "selectSetSelectedIndex"
            | "selectValue"
            | "selectSetValue"
            | "selectUserPick"
            | "selectOptionIds"
            | "optionSelected"
            | "optionSetSelected"
            | "optionDefaultSelected"
            | "outputValue"
            | "outputSetValue"
            | "outputDefaultValue"
            | "outputSetDefault"
            | "formResetControls"
            | "controlWillValidate"
            | "controlSetCustomValidity"
            | "controlPatternVerdict"
            | "controlReportInvalid"
            | "controlValueAsNumber"
            | "controlSetValueAsNumber"
            | "controlStep"
    ) {
        return Ok(None);
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::undefined()));
    };
    let version = node.document_mutation_version();
    let value = match operation {
        "inputValue" => js_string(node.input_value()),
        "inputSetValue" => {
            node.set_input_value(&string_argument(args, 2));
            js_string(node.input_value())
        }
        "inputUserEdit" => JsValue::from(node.user_edit_input(&string_argument(args, 2))),
        "inputDefaultValue" => js_string(node.input_default_value()),
        "textareaValue" => js_string(node.textarea_api_value()),
        "textareaSetValue" => {
            node.set_textarea_raw(&string_argument(args, 2), false);
            js_string(node.textarea_api_value())
        }
        "textareaUserEdit" => JsValue::from(node.set_textarea_raw(&string_argument(args, 2), true)),
        "textareaDefaultValue" => js_string(node.text_content()),
        "selectSelectedIndex" => JsValue::from(node.selected_index() as i32),
        "selectSetSelectedIndex" => {
            set_selected_index(&node, number_argument(args, 2));
            JsValue::from(node.selected_index() as i32)
        }
        "selectValue" => js_string(node.select_value()),
        "selectSetValue" => {
            set_select_value(&node, &string_argument(args, 2));
            js_string(node.select_value())
        }
        "selectUserPick" => JsValue::from(node.user_pick_option(&string_argument(args, 2))),
        "selectOptionIds" => {
            let ids = node
                .select_options()
                .iter()
                .map(|option| state.id_for(option).to_string())
                .collect::<Vec<_>>()
                .join(",");
            js_string(ids)
        }
        "optionSelected" => JsValue::from(node.control_state_snapshot().selectedness),
        "optionSetSelected" => {
            let selected = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            node.update_control_state(|state| {
                state.selectedness = selected;
                state.selected_dirty = true;
            });
            if selected {
                node.enforce_single_select(&node);
            }
            if let Some(select) = node.nearest_select() {
                select.run_selectedness_setting();
                select.update_control_state(|state| {
                    state.reported = false;
                });
            }
            JsValue::undefined()
        }
        "optionDefaultSelected" => JsValue::from(node.attr("selected").is_some()),
        "outputValue" => js_string(node.text_content()),
        "outputSetValue" => {
            let value = string_argument(args, 2);
            // The value setter preserves the current default as the override.
            let current_default = node
                .control_state_snapshot()
                .default_override
                .clone()
                .unwrap_or_else(|| node.text_content());
            node.update_control_state(|state| {
                state.default_override = Some(current_default);
            });
            Node::set_text_content(&node, &value);
            JsValue::undefined()
        }
        "outputDefaultValue" => js_string(
            node.control_state_snapshot()
                .default_override
                .clone()
                .unwrap_or_else(|| node.text_content()),
        ),
        "outputSetDefault" => {
            let value = string_argument(args, 2);
            if node.control_state_snapshot().default_override.is_none() {
                Node::set_text_content(&node, &value);
            } else {
                node.update_control_state(|state| {
                    state.default_override = Some(value.clone());
                });
            }
            JsValue::undefined()
        }
        "formResetControls" => {
            let document = state.document.clone();
            Node::reset_owned_controls(&node, &document);
            JsValue::undefined()
        }
        "controlWillValidate" => JsValue::from(control_validity::will_validate(&node)),
        "controlSetCustomValidity" => {
            node.set_custom_message(&string_argument(args, 2));
            JsValue::undefined()
        }
        "controlPatternVerdict" => {
            // Stores a scripted pattern verdict for scopeless consumers
            // (selector matching); regexes never evaluate in this op.
            let verdict = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let pattern = string_argument(args, 3);
            let values: Vec<String> = args
                .get(4)
                .map(JsValue::string_value)
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            if control_validity::store_pattern_verdict(&node, &pattern, values, verdict) {
                let document = state.document.clone();
                state.record_mutation(Some(&document), MutationKind::State);
            }
            JsValue::undefined()
        }
        "controlReportInvalid" => {
            // Marks interactive reporting; visible feedback follows.
            node.update_control_state(|state| {
                state.reported = true;
            });
            JsValue::undefined()
        }
        "controlValueAsNumber" => JsValue::from(value_as_number(&node)),
        "controlSetValueAsNumber" => js_string(set_value_as_number(
            &node,
            args.get(2).and_then(JsValue::as_number).unwrap_or(f64::NAN),
        )),
        "controlStep" => js_string(step_control(
            &node,
            args.get(2).and_then(JsValue::as_number).unwrap_or(1.0),
            args.get(3).and_then(JsValue::as_boolean).unwrap_or(true),
        )),
        _ => unreachable!(),
    };
    if node.document_mutation_version() != version {
        let document = state.document.clone();
        state.record_mutation(Some(&document), MutationKind::State);
    }
    Ok(Some(value))
}

fn string_argument(args: &[JsValue], index: usize) -> String {
    argument_string(args, index).unwrap_or_default()
}

/// Value as a number; NaN when the API does not apply or has no value.
fn value_as_number(node: &Node) -> f64 {
    if node.tag_name() != Some("input") {
        return f64::NAN;
    }
    // Number/range only: temporal states keep their documented boundary
    // (no date parsing in this experiment).
    if !matches!(node.input_state_name().as_str(), "number" | "range") {
        return f64::NAN;
    }
    control_values_float(&node.input_value()).unwrap_or(f64::NAN)
}

fn control_values_float(value: &str) -> Option<f64> {
    control_values::parse_float_value(value)
}

/// valueAsNumber setter with spec-ordered errors, as a status document.
fn set_value_as_number(node: &Node, value: f64) -> String {
    if value.is_infinite() {
        return String::from("{\"status\":\"type\"}");
    }
    if node.tag_name() != Some("input")
        || !matches!(node.input_state_name().as_str(), "number" | "range")
    {
        return String::from("{\"status\":\"state\"}");
    }
    if value.is_nan() {
        node.set_input_value("");
    } else {
        node.set_input_value(&control_values::number_to_string(value));
    }
    String::from("{\"status\":\"ok\"}")
}

/// stepUp/stepDown with spec-ordered errors, as a status document.
fn step_control(node: &Node, n: f64, up: bool) -> String {
    let state_error = String::from("{\"status\":\"state\"}");
    if node.tag_name() != Some("input")
        || !matches!(node.input_state_name().as_str(), "number" | "range")
    {
        return state_error;
    }
    let state = node.input_state_name();
    let Some(step) = control_numeric::allowed_step(&state, &node.attr("step")) else {
        return state_error;
    };
    let base = control_numeric::step_base(&node.attr("min"), &node.attr("value"));
    let steps = n.trunc() as i64;
    match control_numeric::step_by(
        &node.input_value(),
        &node.attr("min"),
        &node.attr("max"),
        step,
        base,
        steps,
        up,
    ) {
        control_numeric::StepOutcome::Stepped(value) => {
            node.set_input_value(&value);
            String::from("{\"status\":\"ok\"}")
        }
        control_numeric::StepOutcome::NoChange => String::from("{\"status\":\"ok\"}"),
    }
}

fn number_argument(args: &[JsValue], index: usize) -> i64 {
    args.get(index)
        .and_then(JsValue::as_number)
        .map(|number| number.trunc() as i64)
        .unwrap_or(0)
}

/// Sets the selected index: clears all, then selects the match (if any) with
/// dirtiness. Out-of-range values yield no selection, even for single-select.
fn set_selected_index(select: &Node, index: i64) {
    let mut matched = false;
    for (position, option) in select.select_options().iter().enumerate() {
        let is_match = !matched && position as i64 == index;
        option.update_control_state(|state| {
            state.selectedness = is_match;
            if is_match {
                state.selected_dirty = true;
            }
        });
        matched |= is_match;
    }
    select.update_control_state(|state| {
        state.reported = false;
    });
}

/// Value setter: clears all, then selects the first option with a matching
/// value (unmatched values yield no selection).
fn set_select_value(select: &Node, value: &str) {
    let mut matched = false;
    for option in select.select_options() {
        let is_match = !matched && option.option_value() == value;
        option.update_control_state(|state| {
            state.selectedness = is_match;
            if is_match {
                state.selected_dirty = true;
            }
        });
        matched |= is_match;
    }
    select.update_control_state(|state| {
        state.reported = false;
    });
}
