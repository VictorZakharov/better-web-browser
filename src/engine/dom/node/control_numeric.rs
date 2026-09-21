//! Decimal-exact numeric constraint machinery for number/range inputs.
//!
//! Step alignment uses integer arithmetic on decimal mantissas, so common
//! decimal steps (0.1/0.2/0.3, negative values, exponents) decide exactly.
//! Only magnitudes overflowing `i128` scaling fall back to `f64`, which is
//! documented at the call site. Mirrors the platform stepping rules:
//! https://html.spec.whatwg.org/multipage/input.html#attr-input-step

use super::control_values::parse_float_value;

/// Exact decimal: `mantissa * 10^exp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    mantissa: i128,
    exp: i32,
}

/// Parses float-grammar strings exactly (finite only).
pub(crate) fn parse_decimal(text: &str) -> Option<Decimal> {
    let value = parse_float_value(text)?;
    // Re-derive the decimal from the authored string, not the binary float.
    let trimmed = text.trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'));
    let (body, declared) = split_exponent(trimmed)?;
    let negative = body.starts_with('-');
    let digits_body = body.trim_start_matches(['+', '-']);
    let (before, after) = digits_body.split_once('.').unwrap_or((digits_body, ""));
    if before.is_empty() && after.is_empty() {
        return None;
    }
    let mut digits = format!("{before}{after}");
    digits = digits.trim_start_matches('0').to_string();
    let mut exp = declared - after.len() as i32;
    // Strip trailing zeros into the exponent.
    while digits.ends_with('0') && digits.len() > 1 {
        digits.pop();
        exp += 1;
    }
    if digits.is_empty() {
        return Some(Decimal {
            mantissa: 0,
            exp: 0,
        });
    }
    let mut mantissa: i128 = digits.parse().ok()?;
    if negative {
        mantissa = mantissa.checked_neg()?;
    }
    let _ = value;
    Some(Decimal { mantissa, exp })
}

fn split_exponent(text: &str) -> Option<(&str, i32)> {
    let mut body = text;
    let mut declared = 0;
    if let Some(position) = text.find(['e', 'E']) {
        body = &text[..position];
        declared = text[position + 1..].parse::<i32>().ok()?;
    }
    Some((body, declared))
}

fn scale_up(mantissa: i128, steps: u32) -> Option<i128> {
    let mut scaled = mantissa;
    for _ in 0..steps {
        scaled = scaled.checked_mul(10)?;
    }
    Some(scaled)
}

impl Decimal {
    /// Scales both mantissas to the finer (minimum) exponent.
    fn align(self, other: Decimal) -> Option<(i128, i128)> {
        if self.exp == other.exp {
            return Some((self.mantissa, other.mantissa));
        }
        if self.exp < other.exp {
            let shift = (other.exp - self.exp) as u32;
            Some((self.mantissa, scale_up(other.mantissa, shift)?))
        } else {
            let shift = (self.exp - other.exp) as u32;
            Some((scale_up(self.mantissa, shift)?, other.mantissa))
        }
    }

    fn sub(self, other: Decimal) -> Option<Decimal> {
        let (left, right) = self.align(other)?;
        Some(Decimal {
            mantissa: left.checked_sub(right)?,
            exp: self.exp.min(other.exp),
        })
    }

    fn add_scaled(self, scaled: i128, exp: i32) -> Option<Decimal> {
        let (left, right) = self.align(Decimal {
            mantissa: scaled,
            exp,
        })?;
        Some(Decimal {
            mantissa: left.checked_add(right)?,
            exp: self.exp.min(exp),
        })
    }
}

/// Allowed value step for number/range, or `None` for `step=any`/N/A states.
/// Absent, unparseable, and non-positive steps fall back to the default
/// step of one (the constraints suite pins `step=""` with a fractional
/// value as a mismatch).
pub(crate) fn allowed_step(state: &str, step_attr: &Option<String>) -> Option<Decimal> {
    if !matches!(state, "number" | "range") {
        return None;
    }
    let default = Decimal {
        mantissa: 1,
        exp: 0,
    };
    let Some(raw) = step_attr.as_deref() else {
        return Some(default);
    };
    if raw.trim().eq_ignore_ascii_case("any") {
        return None;
    }
    let step = parse_decimal(raw).unwrap_or(default);
    if step.mantissa <= 0 {
        return Some(default);
    }
    Some(step)
}

/// Step base: valid minimum, else valid `value` attribute, else zero.
pub(crate) fn step_base(min_attr: &Option<String>, value_attr: &Option<String>) -> Decimal {
    if let Some(raw) = min_attr.as_deref()
        && let Some(base) = parse_decimal(raw)
    {
        return base;
    }
    if let Some(raw) = value_attr.as_deref()
        && let Some(base) = parse_decimal(raw)
    {
        return base;
    }
    Decimal {
        mantissa: 0,
        exp: 0,
    }
}

/// True when a computable value is not an integral multiple of step off base.
pub(crate) fn step_mismatch(value: &str, base: Decimal, step: Decimal) -> bool {
    let Some(parsed) = parse_decimal(value) else {
        return false;
    };
    let Some(difference) = parsed.sub(base) else {
        return step_mismatch_float(value, base, step);
    };
    let Some((diff, step_scaled)) = difference.align(step) else {
        return step_mismatch_float(value, base, step);
    };
    diff % step_scaled != 0
}

fn step_mismatch_float(value: &str, base: Decimal, step: Decimal) -> bool {
    // Overflow-only fallback for magnitudes beyond i128 scaling.
    let (Some(actual), Some(base), Some(step)) = (
        parse_float_value(value),
        decimal_to_f64(base),
        decimal_to_f64(step),
    ) else {
        return false;
    };
    ((actual - base) / step).fract() != 0.0
}

fn decimal_to_f64(decimal: Decimal) -> Option<f64> {
    let text = decimal_to_string(decimal);
    let parsed: f64 = text.parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Exact decimal rendering (no binary-float rounding).
pub(crate) fn decimal_to_string(decimal: Decimal) -> String {
    if decimal.mantissa == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    let mut digits = decimal.mantissa.abs().to_string();
    if decimal.mantissa < 0 {
        out.push('-');
    }
    if decimal.exp >= 0 {
        out.push_str(&digits);
        out.push_str(&"0".repeat(decimal.exp as usize));
        return out;
    }
    let point = digits.len() as i32 + decimal.exp;
    if point <= 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-point) as usize));
        out.push_str(&digits);
    } else {
        while digits.len() < point as usize {
            digits.push('0');
        }
        out.push_str(&digits[..point as usize]);
        if point as usize != digits.len() {
            out.push('.');
            out.push_str(&digits[point as usize..]);
        }
    }
    out.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub(crate) enum StepOutcome {
    Stepped(String),
    NoChange,
}

/// Runs the platform stepUp/stepDown algorithm exactly.
pub(crate) fn step_by(
    value: &str,
    min_attr: &Option<String>,
    max_attr: &Option<String>,
    step: Decimal,
    base: Decimal,
    n: i64,
    up: bool,
) -> StepOutcome {
    let minimum = min_attr.as_deref().and_then(parse_decimal);
    let maximum = max_attr.as_deref().and_then(parse_decimal);
    if let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && compare(minimum, maximum) == std::cmp::Ordering::Greater
    {
        return StepOutcome::NoChange;
    }
    // No aligned value inside a closed [min, max] window: silent no-op.
    if let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && let (Some(first), Some(last)) = (
            ceil_multiple(minimum, base, step),
            floor_multiple(maximum, base, step),
        )
        && compare(first, last) == std::cmp::Ordering::Greater
    {
        return StepOutcome::NoChange;
    }
    let before = parse_decimal(value).unwrap_or(Decimal {
        mantissa: 0,
        exp: 0,
    });
    let mut stepped = if is_aligned(before, base, step).unwrap_or(false) {
        let delta = Decimal {
            mantissa: step.mantissa.checked_mul(n as i128).unwrap_or(0),
            exp: step.exp,
        };
        let delta = if up { delta } else { -delta };
        before
            .add_scaled(delta.mantissa, delta.exp)
            .unwrap_or(before)
    } else if up {
        ceil_multiple_decimal(before, base, step).unwrap_or(before)
    } else {
        floor_multiple_decimal(before, base, step).unwrap_or(before)
    };
    if let Some(minimum) = minimum
        && compare(stepped, minimum) == std::cmp::Ordering::Less
    {
        stepped = ceil_multiple_decimal(minimum, base, step).unwrap_or(minimum);
    }
    if let Some(maximum) = maximum
        && compare(stepped, maximum) == std::cmp::Ordering::Greater
    {
        stepped = floor_multiple_decimal(maximum, base, step).unwrap_or(maximum);
    }
    if (!up && compare(stepped, before) == std::cmp::Ordering::Greater)
        || (up && compare(stepped, before) == std::cmp::Ordering::Less)
    {
        return StepOutcome::NoChange;
    }
    StepOutcome::Stepped(decimal_to_string(stepped))
}

fn compare(left: Decimal, right: Decimal) -> std::cmp::Ordering {
    match left.align(right) {
        Some((left, right)) => left.cmp(&right),
        None => {
            let left = decimal_to_f64(left).unwrap_or(0.0);
            let right = decimal_to_f64(right).unwrap_or(0.0);
            left.total_cmp(&right)
        }
    }
}

fn is_aligned(value: Decimal, base: Decimal, step: Decimal) -> Option<bool> {
    let difference = value.sub(base)?;
    let (diff, step_scaled) = difference.align(step)?;
    Some(diff % step_scaled == 0)
}

fn quotients(value: Decimal, base: Decimal, step: Decimal) -> Option<(i128, i128, i32)> {
    let difference = value.sub(base)?;
    let exp = difference.exp.min(step.exp);
    let (diff, step_scaled) = difference.align(step)?;
    Some((diff, step_scaled, exp))
}

/// Smallest aligned value at or above `value`.
fn ceil_multiple_decimal(value: Decimal, base: Decimal, step: Decimal) -> Option<Decimal> {
    let (diff, step_scaled, exp) = quotients(value, base, step)?;
    let steps = diff.div_euclid(step_scaled) + i128::from(diff.rem_euclid(step_scaled) != 0);
    let scaled = steps.checked_mul(step_scaled)?;
    base.add_scaled(scaled, exp)
}

/// Largest aligned value at or below `value`.
fn floor_multiple_decimal(value: Decimal, base: Decimal, step: Decimal) -> Option<Decimal> {
    let (diff, step_scaled, exp) = quotients(value, base, step)?;
    let steps = diff.div_euclid(step_scaled);
    let scaled = steps.checked_mul(step_scaled)?;
    base.add_scaled(scaled, exp)
}

fn ceil_multiple(value: Decimal, base: Decimal, step: Decimal) -> Option<Decimal> {
    ceil_multiple_decimal(value, base, step)
}

fn floor_multiple(value: Decimal, base: Decimal, step: Decimal) -> Option<Decimal> {
    floor_multiple_decimal(value, base, step)
}

impl std::ops::Neg for Decimal {
    type Output = Decimal;

    fn neg(self) -> Decimal {
        Decimal {
            mantissa: self.mantissa.checked_neg().unwrap_or(self.mantissa),
            exp: self.exp,
        }
    }
}

/// Step mismatch for temporal states in the state's unit (days, months,
/// weeks, seconds, seconds). `step=any` never mismatches; absent,
/// unparseable, and non-positive steps fall back to the default step (one
/// day, month, or week; sixty seconds). The base is a valid minimum, else
/// the default base (1970-01-01, 1970-01, 1970-W01, midnight, epoch
/// midnight). Empty and out-of-grammar values never mismatch.
pub(crate) fn temporal_step_mismatch(
    state: &str,
    value: &str,
    min_attr: &Option<String>,
    step_attr: &Option<String>,
) -> bool {
    use super::control_temporal::{default_step_rank, step_rank};
    if value.is_empty() {
        return false;
    }
    let Some(actual) = step_rank(state, value) else {
        return false;
    };
    // Time and datetime-local ranks are millis but steps are seconds.
    let scale: i32 = if state == "time" || state == "datetime-local" {
        3
    } else {
        0
    };
    let default = Decimal {
        mantissa: if scale == 3 { 60_000 } else { 1 },
        exp: 0,
    };
    let step = match step_attr.as_deref() {
        None => default,
        Some(raw) if raw.trim().eq_ignore_ascii_case("any") => return false,
        Some(raw) => match parse_decimal(raw) {
            Some(step) if step.mantissa > 0 => Decimal {
                mantissa: step.mantissa,
                exp: step.exp.checked_add(scale).unwrap_or(step.exp),
            },
            _ => default,
        },
    };
    let base_rank = min_attr
        .as_deref()
        .and_then(|bound| step_rank(state, bound))
        .unwrap_or_else(|| default_step_rank(state));
    let Some(difference) = (Decimal {
        mantissa: actual as i128,
        exp: 0,
    })
    .sub(Decimal {
        mantissa: base_rank as i128,
        exp: 0,
    }) else {
        return false;
    };
    let Some((diff, step_scaled)) = difference.align(step) else {
        return false;
    };
    diff % step_scaled != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_steps_stay_exact() {
        let step = allowed_step("number", &Some("0.1".to_string())).expect("step");
        let base = step_base(&None, &None);
        assert!(!step_mismatch("0.3", base, step));
        assert!(step_mismatch("0.35", base, step));
        assert!(step_mismatch("-0.15", base, step));
        assert!(!step_mismatch("-0.2", base, step));
        // Min defines the base when valid.
        let based = step_base(&Some("0.5".to_string()), &None);
        assert!(!step_mismatch("0.7", based, step));
        assert!(step_mismatch("0.75", based, step));
        // step=any and unknown states have no allowed step.
        assert!(allowed_step("number", &Some("any".to_string())).is_none());
        assert!(allowed_step("text", &Some("1".to_string())).is_none());
    }

    #[test]
    fn stepping_aligns_clamps_and_holds_boundaries() {
        let one = Decimal {
            mantissa: 1,
            exp: 0,
        };
        let zero = Decimal {
            mantissa: 0,
            exp: 0,
        };
        // Unparseable values step from zero.
        assert!(matches!(
            step_by("", &None, &None, one, zero, 1, true),
            StepOutcome::Stepped(value) if value == "1"
        ));
        // Misaligned values snap before stepping counts.
        assert!(matches!(
            step_by("0.35", &None, &None, Decimal { mantissa: 1, exp: -1 }, zero, 1, true),
            StepOutcome::Stepped(value) if value == "0.4"
        ));
        // Clamping snaps to the aligned bound.
        assert!(matches!(
            step_by("9", &Some("0".to_string()), &Some("10".to_string()), Decimal { mantissa: 3, exp: 0 }, zero, 2, true),
            StepOutcome::Stepped(value) if value == "9"
        ));
        // Stepping past a maximum that forces retreat is a silent no-op.
        assert!(matches!(
            step_by("1", &None, &Some("0".to_string()), one, zero, 1, true),
            StepOutcome::NoChange
        ));
        // Reversed bounds never step.
        assert!(matches!(
            step_by(
                "5",
                &Some("10".to_string()),
                &Some("0".to_string()),
                one,
                zero,
                1,
                true
            ),
            StepOutcome::NoChange
        ));
    }
}
