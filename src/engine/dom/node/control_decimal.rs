//! Bounded decimal arithmetic shared by form numeric constraints and stepping.
use super::control_values::parse_float_value;

/// Exact decimal: `mantissa * 10^exp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    pub(super) mantissa: i128,
    pub(super) exp: i32,
}

/// Parses float-grammar strings exactly (finite only).
pub(crate) fn parse_decimal(text: &str) -> Option<Decimal> {
    let value = parse_float_value(text)?;
    // Underflow is a finite zero in the input's binary numeric domain. Avoid
    // scaling arbitrarily large negative authored exponents or overflowing i32.
    if value == 0.0 {
        return Some(Decimal {
            mantissa: 0,
            exp: 0,
        });
    }
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
    let mut exp = declared.checked_sub(i32::try_from(after.len()).ok()?)?;
    // Strip trailing zeros into the exponent.
    while digits.ends_with('0') && digits.len() > 1 {
        digits.pop();
        exp = exp.checked_add(1)?;
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
    if mantissa == 0 {
        return Some(0);
    }
    if steps > 38 {
        return None;
    }
    let mut scaled = mantissa;
    for _ in 0..steps {
        scaled = scaled.checked_mul(10)?;
    }
    Some(scaled)
}

impl Decimal {
    /// Scales both mantissas to the finer (minimum) exponent.
    pub(super) fn align(self, other: Decimal) -> Option<(i128, i128)> {
        if self.exp == other.exp {
            return Some((self.mantissa, other.mantissa));
        }
        if self.exp < other.exp {
            let shift = u32::try_from(i64::from(other.exp) - i64::from(self.exp)).ok()?;
            Some((self.mantissa, scale_up(other.mantissa, shift)?))
        } else {
            let shift = u32::try_from(i64::from(self.exp) - i64::from(other.exp)).ok()?;
            Some((scale_up(self.mantissa, shift)?, other.mantissa))
        }
    }

    pub(super) fn sub(self, other: Decimal) -> Option<Decimal> {
        let (left, right) = self.align(other)?;
        Some(Decimal {
            mantissa: left.checked_sub(right)?,
            exp: self.exp.min(other.exp),
        })
    }

    pub(super) fn add_scaled(self, scaled: i128, exp: i32) -> Option<Decimal> {
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

pub(super) fn decimal_to_f64(decimal: Decimal) -> Option<f64> {
    let text = format!("{}e{}", decimal.mantissa, decimal.exp);
    let parsed: f64 = text.parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Exact decimal rendering (no binary-float rounding).
pub(crate) fn decimal_to_string(decimal: Decimal) -> String {
    if decimal.mantissa == 0 {
        return "0".to_string();
    }
    // Scientific notation bounds allocation independently of an authored exponent.
    if !(-64..=64).contains(&decimal.exp) {
        return format!("{}e{}", decimal.mantissa, decimal.exp);
    }
    let mut out = String::new();
    let mut digits = decimal.mantissa.unsigned_abs().to_string();
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
