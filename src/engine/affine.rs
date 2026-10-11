//! Shared two-dimensional CSS geometry; use the already-vetted kurbo matrix implementation.
//! Column-vector composition is `parent * child`, not addition of visual bounds.
use super::layout::RectF;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2D(pub [f64; 6]);

impl Default for Affine2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2D {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(x: f64, y: f64) -> Self {
        Self(kurbo::Affine::translate((x, y)).as_coeffs())
    }

    pub fn then_local(self, local: Self) -> Self {
        Self((kurbo::Affine::new(self.0) * kurbo::Affine::new(local.0)).as_coeffs())
    }

    pub fn is_finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    pub fn inverse(self) -> Option<Self> {
        let matrix = kurbo::Affine::new(self.0);
        if !self.is_finite() || matrix.determinant() == 0.0 {
            return None;
        }
        let inverse = Self(matrix.inverse().as_coeffs());
        inverse.is_finite().then_some(inverse)
    }

    pub fn point(self, x: f64, y: f64) -> (f64, f64) {
        let point = kurbo::Affine::new(self.0) * kurbo::Point::new(x, y);
        (point.x, point.y)
    }

    pub fn bounds(self, rect: RectF) -> RectF {
        let rect = kurbo::Rect::new(
            f64::from(rect.x),
            f64::from(rect.y),
            f64::from(rect.right()),
            f64::from(rect.bottom()),
        );
        let result = kurbo::Affine::new(self.0).transform_rect_bbox(rect);
        RectF {
            x: result.x0 as f32,
            y: result.y0 as f32,
            width: result.width() as f32,
            height: result.height() as f32,
        }
    }

    pub fn translation(self) -> Option<(f32, f32)> {
        (self.0[..4] == [1.0, 0.0, 0.0, 1.0] && self.is_finite())
            .then_some((self.0[4] as f32, self.0[5] as f32))
    }
}

#[cfg(test)]
mod tests;
