//! Keep independent length/percentage bases until the real reference box is known.
use super::math::Expression;
use super::*;
use std::sync::Arc;

pub(crate) fn blend(a: &Length, b: &Length, progress: f64) -> Length {
    Length::Math(Arc::new(Expression::Sum(
        Arc::new(Expression::Scale(
            Arc::new(Expression::Value(a.clone())),
            (1.0 - progress) as f32,
        )),
        Arc::new(Expression::Scale(
            Arc::new(Expression::Value(b.clone())),
            progress as f32,
        )),
    )))
}
