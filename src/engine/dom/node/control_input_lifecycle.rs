//! Input value-mode transitions and range sanitization across all write paths.
use super::Node;
use super::control_numeric::round_range_to_step;
use super::control_values::{
    InputValueMode, canonical_input_state, input_value_mode, number_to_string, parse_float_value,
    sanitize_input_value,
};

impl Node {
    pub(super) fn sanitize_control_value(&self, input_type: &str, value: &str) -> String {
        let sanitized = sanitize_input_value(input_type, value);
        if input_type != "range" {
            return sanitized;
        }
        let minimum = self
            .attr("min")
            .as_deref()
            .and_then(parse_float_value)
            .unwrap_or(0.0);
        let maximum = self
            .attr("max")
            .as_deref()
            .and_then(parse_float_value)
            .unwrap_or(100.0);
        // Halving before adding avoids overflowing opposite finite bounds.
        let default = if maximum < minimum {
            minimum
        } else {
            minimum / 2.0 + maximum / 2.0
        };
        let mut actual = parse_float_value(&sanitized)
            .unwrap_or(default)
            .max(minimum);
        // HTML range overflow clamping is inapplicable to reversed bounds.
        if maximum >= minimum {
            actual = actual.min(maximum);
        }
        number_to_string(round_range_to_step(
            actual,
            minimum,
            maximum,
            &self.attr("min"),
            &self.attr("value"),
            &self.attr("step"),
        ))
    }

    pub(super) fn apply_type_transition(&self) {
        let snapshot = self.control_state_snapshot();
        let previous = snapshot.type_seen.unwrap_or_default();
        let current = canonical_input_state(&self.attr("type").unwrap_or_default());
        if previous == current {
            return;
        }
        let previous_mode = input_value_mode(&previous);
        let current_mode = input_value_mode(&current);
        let live = self.input_value();
        let value_mode = current_mode == InputValueMode::Value;
        let next_value = if value_mode {
            let source = if previous_mode == InputValueMode::Value {
                live.clone()
            } else {
                self.attr("value").unwrap_or_default()
            };
            Some(self.sanitize_control_value(&current, &source))
        } else {
            None
        };
        self.update_control_state_tracked(|state| {
            state.type_seen = Some(current);
            state.value = next_value;
            state.editing = None;
            state.reported = false;
            if previous_mode != InputValueMode::Value || !value_mode {
                state.dirty = false;
                state.user_edited = false;
            }
        });
        if previous_mode == InputValueMode::Value
            && !live.is_empty()
            && matches!(
                current_mode,
                InputValueMode::Default | InputValueMode::DefaultOn | InputValueMode::ButtonDefault
            )
        {
            self.set_attr("value", &live);
        }
    }
}
