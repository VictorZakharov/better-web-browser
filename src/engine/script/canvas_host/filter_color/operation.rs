//! Matrices from Filter Effects' color-function equivalents. This is small
//! specification-derived glue, not a shader/compiler or a copied color library.
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Operation {
    name: Name,
    value: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Name {
    Brightness,
    Contrast,
    Invert,
    Opacity,
    Grayscale,
    Saturate,
    Sepia,
    HueRotate,
}

pub(super) enum Step {
    Transfer { scale: f64, bias: f64, alpha: f64 },
    Matrix([[f64; 3]; 3]),
}

impl Operation {
    pub(super) fn prepare(&self) -> Option<Step> {
        let value = self.value;
        // The CSS admission layer emits finite f32 values. Keep the private
        // transport in that domain too, so matrix arithmetic cannot overflow.
        if !value.is_finite()
            || value.abs() > f64::from(f32::MAX)
            || (!matches!(self.name, Name::HueRotate) && value < 0.0)
        {
            return None;
        }
        let bounded = value.min(1.0);
        Some(match self.name {
            Name::Brightness => Step::Transfer {
                scale: value,
                bias: 0.0,
                alpha: 1.0,
            },
            Name::Contrast => Step::Transfer {
                scale: value,
                bias: 0.5 - 0.5 * value,
                alpha: 1.0,
            },
            Name::Invert => Step::Transfer {
                scale: 1.0 - 2.0 * bounded,
                bias: bounded,
                alpha: 1.0,
            },
            Name::Opacity => Step::Transfer {
                scale: 1.0,
                bias: 0.0,
                alpha: bounded,
            },
            Name::Grayscale => Step::Matrix(saturation(1.0 - bounded, [0.2126, 0.7152, 0.0722])),
            Name::Saturate => Step::Matrix(saturation(value, [0.213, 0.715, 0.072])),
            Name::Sepia => {
                let mut matrix = [
                    [0.393, 0.769, 0.189],
                    [0.349, 0.686, 0.168],
                    [0.272, 0.534, 0.131],
                ];
                for (row, coefficients) in matrix.iter_mut().enumerate() {
                    for (column, coefficient) in coefficients.iter_mut().enumerate() {
                        *coefficient *= bounded;
                        if row == column {
                            *coefficient += 1.0 - bounded;
                        }
                    }
                }
                Step::Matrix(matrix)
            }
            Name::HueRotate => {
                let (sine, cosine) = value.sin_cos();
                Step::Matrix([
                    [
                        0.213 + 0.787 * cosine - 0.213 * sine,
                        0.715 - 0.715 * cosine - 0.715 * sine,
                        0.072 - 0.072 * cosine + 0.928 * sine,
                    ],
                    [
                        0.213 - 0.213 * cosine + 0.143 * sine,
                        0.715 + 0.285 * cosine + 0.140 * sine,
                        0.072 - 0.072 * cosine - 0.283 * sine,
                    ],
                    [
                        0.213 - 0.213 * cosine - 0.787 * sine,
                        0.715 - 0.715 * cosine + 0.715 * sine,
                        0.072 + 0.928 * cosine + 0.072 * sine,
                    ],
                ])
            }
        })
    }
}

fn saturation(scale: f64, weights: [f64; 3]) -> [[f64; 3]; 3] {
    let mut matrix = [[0.0; 3]; 3];
    for (row, coefficients) in matrix.iter_mut().enumerate() {
        for (column, coefficient) in coefficients.iter_mut().enumerate() {
            *coefficient = weights[column] * (1.0 - scale);
            if row == column {
                *coefficient += scale;
            }
        }
    }
    matrix
}

impl Step {
    pub(super) fn apply(&self, input: [f64; 4]) -> [f64; 4] {
        let mut output = input;
        match self {
            Self::Transfer { scale, bias, alpha } => {
                for channel in 0..3 {
                    output[channel] = (input[channel] * scale + bias).clamp(0.0, 1.0);
                }
                output[3] = (input[3] * alpha).clamp(0.0, 1.0);
            }
            Self::Matrix(matrix) => {
                for channel in 0..3 {
                    output[channel] = (matrix[channel][0] * input[0]
                        + matrix[channel][1] * input[1]
                        + matrix[channel][2] * input[2])
                        .clamp(0.0, 1.0);
                }
            }
        }
        output
    }
}
