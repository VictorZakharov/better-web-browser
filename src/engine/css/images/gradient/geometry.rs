//! Gradient line construction and color-stop fixup (CSS Images 3 §3.4).
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Solid(Color),
    Linear {
        start: [f32; 2],
        end: [f32; 2],
    },
    Radial {
        center: [f32; 2],
        radii: [f32; 2],
        start_radius: f32,
        end_radius: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGradient {
    pub geometry: Geometry,
    pub stops: Vec<(f32, Color)>,
    pub repeating: bool,
}

impl Gradient {
    pub(crate) fn resolve(
        &self,
        width: f32,
        height: f32,
        font: f32,
        scale: f32,
        current: Color,
    ) -> Option<ResolvedGradient> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let (mut geometry, basis, flat_radial) = match &self.shape {
            Shape::Linear(direction) => {
                let (dx, dy) = match direction {
                    Direction::Angle(degrees) => {
                        let rad = degrees.to_radians();
                        (rad.sin(), -rad.cos())
                    }
                    Direction::Corner { x, y } => {
                        let (dx, dy) = if *x == 0.0 || *y == 0.0 {
                            (*x, *y)
                        } else {
                            (*x * height, *y * width)
                        };
                        let norm = dx.hypot(dy);
                        (dx / norm, dy / norm)
                    }
                };
                let line = (width * dx).abs() + (height * dy).abs();
                let center = [width / 2.0, height / 2.0];
                (
                    Geometry::Linear {
                        start: [center[0] - dx * line / 2.0, center[1] - dy * line / 2.0],
                        end: [center[0] + dx * line / 2.0, center[1] + dy * line / 2.0],
                    },
                    line,
                    false,
                )
            }
            Shape::Radial {
                circle,
                center,
                size,
            } => {
                let center = [
                    zoom::length(&center[0], scale).resolve(width, font)?,
                    zoom::length(&center[1], scale).resolve(height, font)?,
                ];
                let (radii, flat) = size.resolve([width, height], center, font, scale, *circle)?;
                (
                    Geometry::Radial {
                        center,
                        radii,
                        start_radius: 0.0,
                        end_radius: 1.0,
                    },
                    radii[0],
                    flat,
                )
            }
        };
        if basis <= 0.0 || !basis.is_finite() {
            return None;
        }
        let mut positions = self
            .stops
            .iter()
            .map(|stop| {
                stop.position
                    .as_ref()
                    .map(|p| {
                        zoom::length(p, scale)
                            .resolve(basis, font)
                            .filter(|v| v.is_finite())
                    })
                    .map_or(Some(None), |position| position.map(Some))
            })
            .collect::<Option<Vec<_>>>()?;
        positions[0] = Some(positions[0].unwrap_or(0.0));
        let last = positions.len() - 1;
        positions[last] = Some(positions[last].unwrap_or(basis));
        let mut previous = f32::NEG_INFINITY;
        for position in positions.iter_mut().flatten() {
            if !position.is_finite() {
                return None;
            }
            *position = position.max(previous);
            previous = *position;
        }
        let mut start = 0;
        while start < last {
            let end = (start + 1..=last).find(|i| positions[*i].is_some())?;
            let left = positions[start]?;
            let right = positions[end]?;
            for (offset, position) in positions[start + 1..end].iter_mut().enumerate() {
                *position =
                    Some(left + (right - left) * (offset + 1) as f32 / (end - start) as f32);
            }
            start = end;
        }
        let first = positions[0]? / basis;
        let final_stop = positions[last]? / basis;
        let span = final_stop - first;
        if flat_radial {
            return Some(ResolvedGradient {
                geometry: Geometry::Solid(if self.repeating {
                    average_color(&self.stops, &positions, current)
                } else {
                    self.stops.last()?.color.unwrap_or(current)
                }),
                stops: vec![],
                repeating: false,
            });
        }
        if self.repeating && span * basis < 0.25 {
            return Some(ResolvedGradient {
                geometry: Geometry::Solid(average_color(&self.stops, &positions, current)),
                stops: vec![],
                repeating: false,
            });
        }
        if span > 0.0
            && let Geometry::Linear { start, end } = &mut geometry
        {
            let delta = [end[0] - start[0], end[1] - start[1]];
            *end = [
                start[0] + delta[0] * final_stop,
                start[1] + delta[1] * final_stop,
            ];
            *start = [start[0] + delta[0] * first, start[1] + delta[1] * first];
        }
        let mut stops: Vec<_> = self
            .stops
            .iter()
            .zip(positions)
            .map(|(stop, position)| {
                let offset = if span > 0.0 {
                    (position.unwrap() / basis - first) / span
                } else {
                    position.unwrap() / basis
                };
                (offset, stop.color.unwrap_or(current))
            })
            .collect();
        if let Geometry::Radial {
            start_radius,
            end_radius,
            ..
        } = &mut geometry
        {
            if self.repeating {
                *start_radius = first.rem_euclid(span);
                *end_radius = *start_radius + span;
            } else if final_stop <= 0.0 {
                geometry = Geometry::Solid(stops.last()?.1);
                stops.clear();
            } else if first >= 0.0 && span > 0.0 {
                *start_radius = first;
                *end_radius = final_stop;
            } else {
                // Negative radial stops contribute to the center color, but negative
                // radii do not exist. Clip that prefix with premultiplied interpolation.
                let zero = if span > 0.0 { -first / span } else { 0.0 };
                let center = color_at(&stops, zero);
                stops = std::iter::once((0.0, center))
                    .chain(
                        stops
                            .into_iter()
                            .filter(|(offset, _)| *offset > zero)
                            .map(|(offset, color)| ((first + offset * span) / final_stop, color)),
                    )
                    .collect();
                *start_radius = 0.0;
                *end_radius = final_stop;
            }
        }
        Some(ResolvedGradient {
            geometry,
            stops,
            repeating: self.repeating,
        })
    }
}

fn color_at(stops: &[(f32, Color)], offset: f32) -> Color {
    let Some(index) = stops.iter().position(|(position, _)| *position > offset) else {
        return stops.last().unwrap().1;
    };
    if index == 0 {
        return stops[0].1;
    }
    let (a, left) = stops[index - 1];
    let (b, right) = stops[index];
    mix(left, right, ((offset - a) / (b - a)).clamp(0.0, 1.0))
}

fn mix(left: Color, right: Color, weight: f32) -> Color {
    let alpha = f32::from(left.alpha) * (1.0 - weight) + f32::from(right.alpha) * weight;
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |a: u8, b: u8| {
        ((f32::from(a) * f32::from(left.alpha) * (1.0 - weight)
            + f32::from(b) * f32::from(right.alpha) * weight)
            / alpha)
            .round() as u8
    };
    Color {
        red: channel(left.red, right.red),
        green: channel(left.green, right.green),
        blue: channel(left.blue, right.blue),
        alpha: alpha.round() as u8,
    }
}

fn average_color(stops: &[Stop], positions: &[Option<f32>], current: Color) -> Color {
    let span = positions.last().unwrap().unwrap() - positions[0].unwrap();
    let mut total = [0.0; 4];
    for (index, pair) in stops.windows(2).enumerate() {
        let weight = if span > 0.0 {
            (positions[index + 1].unwrap() - positions[index].unwrap()) / span / 2.0
        } else {
            0.5 / (stops.len() - 1) as f32
        };
        for stop in pair {
            let color = stop.color.unwrap_or(current);
            let alpha = f32::from(color.alpha) / 255.0;
            for (sum, channel) in total.iter_mut().zip([
                f32::from(color.red) * alpha,
                f32::from(color.green) * alpha,
                f32::from(color.blue) * alpha,
                f32::from(color.alpha),
            ]) {
                *sum += channel * weight;
            }
        }
    }
    if total[3] == 0.0 {
        return Color::TRANSPARENT;
    }
    Color {
        red: (total[0] * 255.0 / total[3]).round() as u8,
        green: (total[1] * 255.0 / total[3]).round() as u8,
        blue: (total[2] * 255.0 / total[3]).round() as u8,
        alpha: total[3].round() as u8,
    }
}
