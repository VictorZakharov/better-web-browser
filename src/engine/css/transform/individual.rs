//! CSS Transforms 2: individual `translate`, before the transform list.
//! Nonzero Z is rejected until the painter has a 3D transform path.
//! https://drafts.csswg.org/css-transforms-2/#individual-transforms
use super::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct IndividualTranslate(pub(super) Option<[Length; 2]>);

impl IndividualTranslate {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("none") {
            return Some(Self::default());
        }
        let parts = borrowed_components(value)?;
        if !(1..=3).contains(&parts.len()) {
            return None;
        }
        let length = |value: &str| {
            let value = parse_length(value)?;
            (value != Length::Auto).then_some(value)
        };
        let x = length(parts[0])?;
        let y = parts.get(1).map_or(Some(Length::Px(0.0)), |v| length(v))?;
        if let Some(z) = parts.get(2) {
            let z = length(z)?;
            if !matches!(z, Length::Px(0.0)) {
                return None;
            }
        }
        Some(Self(Some([x, y])))
    }

    pub(crate) fn css_text(&self) -> String {
        let Some([x, y]) = &self.0 else {
            return "none".into();
        };
        if *y == Length::Px(0.0) {
            serialize_length(x.clone())
        } else {
            format!(
                "{} {}",
                serialize_length(x.clone()),
                serialize_length(y.clone())
            )
        }
    }

    pub(crate) fn map_lengths(&mut self, map: impl Fn(Length) -> Length) {
        if let Some(values) = &mut self.0 {
            *values = values.clone().map(map);
        }
    }
}

#[cfg(test)]
mod tests;

impl ComputedStyle {
    pub(crate) fn has_transform(&self) -> bool {
        self.translate.0.is_some() || !self.transform.is_none()
    }

    pub(crate) fn translation_transform(&self) -> TransformList {
        let mut result = self.transform.clone();
        if let Some([x, y]) = &self.translate.0 {
            result.0.insert(
                0,
                TranslateOperation {
                    x: x.clone(),
                    y: y.clone(),
                },
            );
        }
        result
    }
}

pub(crate) fn interpolate(from: &str, to: &str, progress: f64) -> Option<String> {
    let first = IndividualTranslate::parse(from)?;
    let last = IndividualTranslate::parse(to)?;
    let as_transform = |value: &IndividualTranslate| {
        value.0.as_ref().map_or_else(
            || "none".into(),
            |[x, y]| {
                format!(
                    "translate({}, {})",
                    serialize_length(x.clone()),
                    serialize_length(y.clone())
                )
            },
        )
    };
    let result = super::interpolate(&as_transform(&first), &as_transform(&last), progress)?;
    let transform = parser::parse_transform(&result)?;
    let Some(operation) = transform.0.first() else {
        return Some("none".into());
    };
    Some(IndividualTranslate(Some([operation.x.clone(), operation.y.clone()])).css_text())
}
