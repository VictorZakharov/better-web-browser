use super::*;

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
