//! Matrix decomposition for absolute 2D tails, following CSS Transforms 1 §13.2.
//! Relative mixed tails need a used-box interpolation checkpoint, not a guessed zero basis.
use super::*;
use crate::engine::affine::Affine2D;

struct Decomposed {
    translation: [f64; 2],
    scale: [f64; 2],
    angle: f64,
    residual: [f64; 4],
}

pub(super) fn interpolate(a: &[Operation], b: &[Operation], p: f64) -> Option<Vec<Operation>> {
    let mut a = decompose(absolute(a)?)?;
    let b = decompose(absolute(b)?)?;
    if (a.scale[0] < 0.0 && b.scale[1] < 0.0) || (a.scale[1] < 0.0 && b.scale[0] < 0.0) {
        a.scale = a.scale.map(|v| -v);
        a.angle += if a.angle < 0.0 {
            std::f64::consts::PI
        } else {
            -std::f64::consts::PI
        };
    }
    let mut angles = [a.angle, b.angle];
    let turn = std::f64::consts::TAU;
    for v in &mut angles {
        if *v == 0.0 {
            *v = turn;
        }
    }
    if (angles[0] - angles[1]).abs() > std::f64::consts::PI {
        if angles[0] > angles[1] {
            angles[0] -= turn;
        } else {
            angles[1] -= turn;
        }
    }
    let mix = |a, b| a * (1.0 - p) + b * p;
    let [x, y] = [0, 1].map(|i| mix(a.translation[i], b.translation[i]));
    let scale = [0, 1].map(|i| mix(a.scale[i], b.scale[i]));
    let residual = [0, 1, 2, 3].map(|i| mix(a.residual[i], b.residual[i]));
    let (sin, cos) = mix(angles[0], angles[1]).sin_cos();
    let matrix = Affine2D::translate(x, y)
        .then_local(Affine2D([cos, sin, -sin, cos, 0.0, 0.0]))
        .then_local(Affine2D([
            residual[0],
            residual[1],
            residual[2],
            residual[3],
            0.0,
            0.0,
        ]))
        .then_local(Affine2D([scale[0], 0.0, 0.0, scale[1], 0.0, 0.0]));
    matrix
        .is_finite()
        .then_some(vec![Operation::Matrix(matrix)])
}

fn absolute(operations: &[Operation]) -> Option<Affine2D> {
    for operation in operations {
        if let Operation::Translate(v) = operation {
            let absolute = |length: &Length| match length {
                Length::Px(_) => true,
                Length::Math(expression) => {
                    expression.resolve(None, 0.0).is_some()
                        && expression
                            .map_lengths(&|v| match v {
                                Length::Px(_) => v.clone(),
                                _ => Length::Auto,
                            })
                            .resolve(None, 0.0)
                            .is_some()
                }
                _ => false,
            };
            if !absolute(&v.x) || !absolute(&v.y) {
                return None;
            }
        }
    }
    let matrix = TransformList(operations.to_vec()).matrix(0.0, 0.0, 0.0);
    matrix.is_finite().then_some(matrix)
}

fn decompose(matrix: Affine2D) -> Option<Decomposed> {
    matrix.inverse()?;
    let [a, b, c, d, x, y] = matrix.0;
    let mut scale = [a.hypot(b), c.hypot(d)];
    if a * d - b * c < 0.0 {
        if a < d {
            scale[0] = -scale[0];
        } else {
            scale[1] = -scale[1];
        }
    }
    let [a, b, c, d] = [a / scale[0], b / scale[0], c / scale[1], d / scale[1]];
    let angle = b.atan2(a);
    let (sin, cos) = angle.sin_cos();
    Some(Decomposed {
        translation: [x, y],
        scale,
        angle,
        residual: [
            cos * a + sin * b,
            -sin * a + cos * b,
            cos * c + sin * d,
            -sin * c + cos * d,
        ],
    })
}

#[cfg(test)]
mod tests;
