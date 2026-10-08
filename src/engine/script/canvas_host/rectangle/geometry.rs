//! The existing Canvas double-coordinate area integration, moved out of the
//! per-pixel JavaScript loop. Do not quantize coordinates through f32 masks.

type Point = [f64; 2];

struct Polygon {
    points: [Point; 12],
    length: usize,
}

impl Polygon {
    fn from_rectangle(points: &[Point; 4]) -> Self {
        let mut polygon = Self {
            points: [[0.0; 2]; 12],
            length: 4,
        };
        polygon.points[..4].copy_from_slice(points);
        polygon
    }

    fn push(&mut self, point: Point) -> Option<()> {
        // Each half-plane adds at most two vertices to a convex polygon.
        // Retain coincident intersection vertices just like the scalar path:
        // four initial vertices plus four pairs need at most twelve slots.
        // Decline an unexpected numerically nonconvex packet rather than
        // panicking or dropping vertices to fabricate a partial painted shape.
        if self.length == self.points.len() {
            return None;
        }
        self.points[self.length] = point;
        self.length += 1;
        Some(())
    }

    fn clipped(&self, axis: usize, boundary: f64, greater: bool) -> Option<Self> {
        let mut output = Self {
            points: [[0.0; 2]; 12],
            length: 0,
        };
        if self.length == 0 {
            return Some(output);
        }
        let inside = |point: Point| {
            if greater {
                point[axis] >= boundary
            } else {
                point[axis] <= boundary
            }
        };
        let mut previous = self.points[self.length - 1];
        let mut previous_inside = inside(previous);
        for &current in &self.points[..self.length] {
            let current_inside = inside(current);
            if current_inside != previous_inside {
                let fraction = (boundary - previous[axis]) / (current[axis] - previous[axis]);
                let mut point = [
                    previous[0] + fraction * (current[0] - previous[0]),
                    previous[1] + fraction * (current[1] - previous[1]),
                ];
                point[axis] = boundary;
                output.push(point)?;
            }
            if current_inside {
                output.push(current)?;
            }
            previous = current;
            previous_inside = current_inside;
        }
        Some(output)
    }
}

pub(super) fn area(points: &[Point; 4], x: f64, y: f64) -> Option<f64> {
    let polygon = Polygon::from_rectangle(points)
        .clipped(0, x, true)?
        .clipped(0, x + 1.0, false)?
        .clipped(1, y, true)?
        .clipped(1, y + 1.0, false)?;
    let mut twice_area = 0.0;
    for index in 0..polygon.length {
        let a = polygon.points[index];
        let b = polygon.points[(index + 1) % polygon.length];
        // Preserve the scalar evaluation order and local pixel origin. Large
        // absolute coordinates must not cancel a subpixel intersection away.
        twice_area += (a[0] - x) * (b[1] - y) - (a[1] - y) * (b[0] - x);
    }
    Some((twice_area.abs() / 2.0).min(1.0))
}

pub(super) fn polygon([x, y, width, height]: [f64; 4], [a, b, c, d, e, f]: [f64; 6]) -> [Point; 4] {
    [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ]
    .map(|[x, y]| [a * x + c * y + e, b * x + d * y + f])
}

pub(super) fn bounds(points: &[Point; 4]) -> [f64; 4] {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for [x, y] in points {
        bounds[0] = bounds[0].min(*x);
        bounds[1] = bounds[1].min(*y);
        bounds[2] = bounds[2].max(*x);
        bounds[3] = bounds[3].max(*y);
    }
    bounds
}

#[cfg(test)]
mod tests;
