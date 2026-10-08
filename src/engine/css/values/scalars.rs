//! Number calculations retain their typed tree until the final font/viewport
//! context is known. CSS Values 4 §10.11–12 requires computed-time resolution
//! and range clamping, not evaluation with a guessed initial font size.
use super::super::value_parser::{math as parser, numbers};
use super::*;
mod ratio;
pub(crate) use ratio::RatioValue;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NumberValue {
    Fixed(f32),
    Calculation(Arc<math::Expression>),
}

impl NumberValue {
    pub(crate) fn number(value: &str) -> Option<Self> {
        numbers::number(value)
            .map(Self::Fixed)
            .or_else(|| parser::number_expression(value).map(Self::Calculation))
    }

    pub(crate) fn css_text(&self) -> String {
        match self {
            Self::Fixed(value) => value.to_string(),
            Self::Calculation(expression) => expression.css_text(),
        }
    }
    pub(crate) fn nonnegative(value: &str) -> Option<Self> {
        numbers::nonnegative(value)
            .map(Self::Fixed)
            .or_else(|| parser::number_expression(value).map(Self::Calculation))
    }

    pub(crate) fn opacity(value: &str) -> Option<Self> {
        super::super::value_parser::parse_opacity(value)
            .map(Self::Fixed)
            .or_else(|| parser::number_expression(value).map(Self::Calculation))
    }

    pub(crate) fn assign(self, value: &mut f32, pending: &mut Option<Self>) {
        match self {
            Self::Fixed(fixed) => {
                *value = fixed;
                *pending = None;
            }
            calculation => *pending = Some(calculation),
        }
    }

    pub(crate) fn resolve(&self, font: f32, root: f32, width: f32, height: f32) -> f32 {
        match self {
            Self::Fixed(value) => *value,
            Self::Calculation(expression) => resolve(expression, font, root, width, height)
                .clamp(-f64::from(f32::MAX), f64::from(f32::MAX))
                as f32,
        }
    }
}

fn resolve(
    expression: &Arc<math::Expression>,
    font: f32,
    root: f32,
    width: f32,
    height: f32,
) -> f64 {
    expression
        .map_lengths(&|length| {
            length
                .clone()
                .resolve_root_font_units(root)
                .resolve_viewport_units(width, height)
        })
        .resolve_f64(None, font)
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ScalarCalculations {
    pub(crate) opacity: Option<NumberValue>,
    pub(crate) grow: Option<NumberValue>,
    pub(crate) shrink: Option<NumberValue>,
    pub(crate) z_index: Option<Arc<math::Expression>>,
    pub(crate) ratio: Option<RatioValue>,
}

impl ScalarCalculations {
    pub(in crate::engine::css) fn copy_property(&mut self, source: &Self, property: &str) {
        match property {
            "opacity" => self.opacity.clone_from(&source.opacity),
            "z-index" => self.z_index.clone_from(&source.z_index),
            "aspect-ratio" => self.ratio.clone_from(&source.ratio),
            "flex-grow" | "-webkit-flex-grow" | "-moz-flex-grow" | "-webkit-box-flex" => {
                self.grow.clone_from(&source.grow);
            }
            "flex-shrink" | "-webkit-flex-shrink" | "-moz-flex-shrink" => {
                self.shrink.clone_from(&source.shrink);
            }
            "flex" | "-webkit-flex" | "-moz-flex" => {
                self.grow.clone_from(&source.grow);
                self.shrink.clone_from(&source.shrink);
            }
            _ => {}
        }
    }
}

impl ComputedStyle {
    pub(in crate::engine::css) fn resolve_scalars(&mut self, width: f32, height: f32, root: f32) {
        // Clear authored trees after computing: explicit inheritance copies the
        // parent's computed number, not an expression re-evaluated in the child.
        let pending = std::mem::take(&mut self.scalar_calculations);
        if let Some(value) = pending.ratio {
            self.aspect_ratio = value.resolve(self.font_size, root, width, height);
        }
        if let Some(value) = pending.opacity {
            self.opacity = value
                .resolve(self.font_size, root, width, height)
                .clamp(0.0, 1.0);
        }
        if let Some(value) = pending.grow {
            self.flex_grow = value.resolve(self.font_size, root, width, height).max(0.0);
        }
        if let Some(value) = pending.shrink {
            self.flex_shrink = value.resolve(self.font_size, root, width, height).max(0.0);
        }
        if let Some(expression) = pending.z_index {
            // Keep f64 until integer rounding; adjacent layers above 2^24 must
            // not collapse. CSS integer ties go toward positive infinity.
            self.z_index = Some(
                (resolve(&expression, self.font_size, root, width, height) + 0.5)
                    .floor()
                    .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
            );
        }
    }
}
