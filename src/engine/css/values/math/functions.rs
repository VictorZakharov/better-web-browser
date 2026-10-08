//! CSS Values 4 §§10.3, 10.5, 10.6. Evaluate only after percentages and font /
//! viewport units have their property-specific basis. No early sign inference.
//! https://www.w3.org/TR/css-values-4/#calc-functions

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rounding {
    Nearest,
    Up,
    Down,
    ToZero,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Function {
    Round(Rounding),
    Mod,
    Rem,
    Abs,
    Sign,
    Hypot,
    Pow,
    Sqrt,
    Log,
    Exp,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
}

impl Function {
    pub(crate) fn css_text(self, arguments: &str) -> String {
        let name = match self {
            Self::Round(strategy) => {
                return format!(
                    "round({}, {arguments})",
                    match strategy {
                        Rounding::Nearest => "nearest",
                        Rounding::Up => "up",
                        Rounding::Down => "down",
                        Rounding::ToZero => "to-zero",
                    }
                );
            }
            Self::Mod => "mod",
            Self::Rem => "rem",
            Self::Abs => "abs",
            Self::Sign => "sign",
            Self::Hypot => "hypot",
            Self::Pow => "pow",
            Self::Sqrt => "sqrt",
            Self::Log => "log",
            Self::Exp => "exp",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Asin => "asin",
            Self::Acos => "acos",
            Self::Atan => "atan",
            Self::Atan2 => "atan2",
        };
        format!("{name}({arguments})")
    }

    pub(crate) fn evaluate(self, values: &[f64]) -> Option<f64> {
        if values.iter().any(|value| value.is_nan()) {
            return Some(f64::NAN);
        }
        let a = *values.first()?;
        let b = values.get(1).copied();
        let result = match self {
            Self::Abs => a.abs(),
            Self::Sign => {
                if a == 0.0 {
                    a
                } else {
                    a.signum()
                }
            }
            // Repeated hypot avoids the overflow of sum(x*x) on finite inputs.
            Self::Hypot => values.iter().fold(0.0f64, |sum, value| sum.hypot(*value)),
            Self::Pow => {
                let exponent = b?;
                if a.abs() == 1.0 && exponent.is_infinite() {
                    f64::NAN
                } else {
                    a.powf(exponent)
                }
            }
            Self::Sqrt => a.sqrt(),
            Self::Log => b.map_or_else(
                || a.ln(),
                |base| {
                    if base <= 0.0 || base == 1.0 {
                        f64::NAN
                    } else {
                        a.ln() / base.ln()
                    }
                },
            ),
            Self::Exp => a.exp(),
            Self::Sin | Self::Cos | Self::Tan => trigonometric(self, a),
            Self::Asin => a.asin(),
            Self::Acos => a.acos(),
            Self::Atan => a.atan(),
            Self::Atan2 => a.atan2(b?),
            Self::Rem => {
                let step = b?;
                if a.is_infinite() || step == 0.0 {
                    f64::NAN
                } else if step.is_infinite() {
                    a
                } else {
                    a % step
                }
            }
            Self::Mod => {
                let step = b?;
                if a.is_infinite() || step == 0.0 {
                    return Some(f64::NAN);
                }
                if step.is_infinite() {
                    return Some(if a.is_sign_negative() != step.is_sign_negative() {
                        f64::NAN
                    } else {
                        a
                    });
                }
                let remainder = a % step;
                if remainder == 0.0 {
                    0.0f64.copysign(step)
                } else if remainder.is_sign_negative() != step.is_sign_negative() {
                    remainder + step
                } else {
                    remainder
                }
            }
            Self::Round(strategy) => {
                let step = b?.abs();
                if step == 0.0 {
                    return Some(f64::NAN);
                }
                if a.is_infinite() {
                    return Some(if step.is_infinite() { f64::NAN } else { a });
                }
                if step.is_infinite() {
                    return Some(match strategy {
                        Rounding::Up if a > 0.0 => f64::INFINITY,
                        Rounding::Down if a < 0.0 => f64::NEG_INFINITY,
                        _ => 0.0f64.copysign(a),
                    });
                }
                if a % step == 0.0 {
                    return Some(a);
                }
                let quotient = a / step;
                let multiple = match strategy {
                    Rounding::Nearest => {
                        let lower = quotient.floor();
                        if quotient - lower < 0.5 {
                            lower
                        } else {
                            lower + 1.0
                        }
                    }
                    Rounding::Up => quotient.ceil(),
                    Rounding::Down => quotient.floor(),
                    Rounding::ToZero => quotient.trunc(),
                };
                // Upper zero is -0, lower zero is +0; nearest ties go upward.
                if multiple == 0.0 {
                    0.0f64.copysign(a)
                } else {
                    multiple * step
                }
            }
        };
        Some(result)
    }
}

pub(super) fn minimum(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}

pub(super) fn maximum(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else {
        a.max(b)
    }
}

pub(super) fn serialize_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-infinity".into()
    }
    // Preserve negative zero as math rather than depending on token round trips.
    else if value == 0.0 && value.is_sign_negative() {
        "calc(-1 * 0)".into()
    } else {
        value.to_string()
    }
}

fn trigonometric(function: Function, value: f64) -> f64 {
    // Preserve exact quadrants (90deg / .25turn), without epsilon-snapping
    // nearby author angles. Rust's sin(pi) otherwise leaves a tiny residue.
    let half_pi = std::f64::consts::FRAC_PI_2;
    if value == 0.0 {
        return if function == Function::Cos {
            1.0
        } else {
            value
        };
    }
    if value.is_finite() && value % half_pi == 0.0 {
        let quadrant = (value / half_pi).rem_euclid(4.0);
        return match function {
            Function::Sin => {
                if quadrant == 1.0 {
                    1.0
                } else if quadrant == 3.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Function::Cos => {
                if quadrant == 0.0 {
                    1.0
                } else if quadrant == 2.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            Function::Tan if quadrant == 1.0 => f64::INFINITY,
            Function::Tan if quadrant == 3.0 => f64::NEG_INFINITY,
            _ => 0.0,
        };
    }
    match function {
        Function::Sin => value.sin(),
        Function::Cos => value.cos(),
        _ => value.tan(),
    }
}
