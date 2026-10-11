//! Individual 2D rotate/scale are ordered translate → rotate → scale → transform.
//! None and identity differ: an authored identity still establishes a containing block.
use super::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct IndividualScale(pub(super) Option<[f32; 2]>);

impl IndividualScale {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("none") {
            return Some(Self::default());
        }
        let parts = borrowed_components(value)?;
        let values = match parts.as_slice() {
            [v] => {
                let v = parser::scale(v)?;
                [v, v]
            }
            [x, y] => [parser::scale(x)?, parser::scale(y)?],
            [x, y, z] if parser::scale(z)? == 1.0 => [parser::scale(x)?, parser::scale(y)?],
            _ => return None,
        };
        Some(Self(Some(values)))
    }
    pub(crate) fn css_text(&self) -> String {
        self.0.map_or_else(
            || "none".into(),
            |[x, y]| {
                if x == y {
                    x.to_string()
                } else {
                    format!("{x} {y}")
                }
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct IndividualRotate(pub(super) Option<f32>);

impl IndividualRotate {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("none") {
            return Some(Self::default());
        }
        let parts = borrowed_components(value)?;
        let angle = match parts.as_slice() {
            [v] => parser::angle(v)?,
            [axis, v] if super::super::syntax::ident(axis)?.eq_ignore_ascii_case("z") => {
                parser::angle(v)?
            }
            [x, y, z, v]
                if super::super::value_parser::numbers::number(x)? == 0.0
                    && super::super::value_parser::numbers::number(y)? == 0.0 =>
            {
                let z = super::super::value_parser::numbers::number(z)?;
                let angle = parser::angle(v)?;
                if z == 0.0 { 0.0 } else { angle * z.signum() }
            }
            _ => return None,
        };
        Some(Self(Some(angle)))
    }
    pub(crate) fn css_text(&self) -> String {
        self.0
            .map_or_else(|| "none".into(), |v| format!("{}deg", v.to_degrees()))
    }
}

pub(crate) fn interpolate_scale(from: &str, to: &str, progress: f64) -> Option<String> {
    if !progress.is_finite() {
        return None;
    }
    let a = IndividualScale::parse(from)?.0.unwrap_or([1.0, 1.0]);
    let b = IndividualScale::parse(to)?.0.unwrap_or([1.0, 1.0]);
    let mix = |i| (f64::from(a[i]) * (1.0 - progress) + f64::from(b[i]) * progress) as f32;
    let v = [mix(0), mix(1)];
    v.iter()
        .all(|v| v.is_finite())
        .then(|| IndividualScale(Some(v)).css_text())
}

pub(crate) fn interpolate_rotate(from: &str, to: &str, progress: f64) -> Option<String> {
    if !progress.is_finite() {
        return None;
    }
    let a = IndividualRotate::parse(from)?.0.unwrap_or(0.0);
    let b = IndividualRotate::parse(to)?.0.unwrap_or(0.0);
    let v = (f64::from(a) * (1.0 - progress) + f64::from(b) * progress) as f32;
    v.is_finite().then(|| IndividualRotate(Some(v)).css_text())
}

#[cfg(test)]
mod tests;
