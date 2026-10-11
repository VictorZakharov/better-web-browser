use super::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct Geometry {
    pub(in crate::engine::layout) boxes: BoxMap,
    pub(in crate::engine::layout) clips: BoxMap,
    pub(in crate::engine::layout) ports: BoxMap,
}

impl Geometry {
    pub(in crate::engine::layout) fn remove(&mut self, id: NodeId) {
        self.boxes.0.remove(&id);
        self.clips.0.remove(&id);
        self.ports.0.remove(&id);
    }

    pub(in crate::engine::layout) fn extend(&mut self, other: Self) {
        self.boxes.0.extend(other.boxes.0);
        self.clips.0.extend(other.clips.0);
        self.ports.0.extend(other.ports.0);
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine::layout) struct BoxMap(HashMap<NodeId, TransformedBox>);

#[derive(Debug, Clone, Copy)]
struct TransformedBox {
    local: RectF,
    matrix: Affine2D,
}

impl BoxMap {
    pub(in crate::engine::layout) fn vector(&self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        let Some(value) = self.0.get(&id) else {
            return (x, y);
        };
        let [a, b, c, d, _, _] = value.matrix.0;
        (
            (a * f64::from(x) + c * f64::from(y)) as f32,
            (b * f64::from(x) + d * f64::from(y)) as f32,
        )
    }

    pub(in crate::engine::layout) fn apply(
        &mut self,
        id: NodeId,
        rect: RectF,
        matrix: Affine2D,
    ) -> RectF {
        let value = self.0.entry(id).or_insert(TransformedBox {
            local: rect,
            matrix: Affine2D::IDENTITY,
        });
        value.matrix = matrix.then_local(value.matrix);
        value.matrix.bounds(value.local)
    }

    pub(in crate::engine::layout) fn translate(&mut self, id: NodeId, x: f32, y: f32) {
        if let Some(value) = self.0.get_mut(&id) {
            value.local.x += x;
            value.local.y += y;
            value.matrix = Affine2D::translate(f64::from(x), f64::from(y))
                .then_local(value.matrix)
                .then_local(Affine2D::translate(-f64::from(x), -f64::from(y)));
        }
    }

    fn point(&self, id: NodeId, x: f32, y: f32) -> Option<(RectF, f64, f64)> {
        let value = self.0.get(&id)?;
        let (x, y) = value.matrix.inverse()?.point(f64::from(x), f64::from(y));
        Some((value.local, x, y))
    }

    pub(in crate::engine::layout) fn contains(
        &self,
        id: NodeId,
        x: f32,
        y: f32,
        clip_x: bool,
        clip_y: bool,
    ) -> Option<bool> {
        self.0.get(&id)?;
        Some(self.point(id, x, y).is_some_and(|(rect, x, y)| {
            (!clip_x || (x >= f64::from(rect.x) && x < f64::from(rect.right())))
                && (!clip_y || (y >= f64::from(rect.y) && y < f64::from(rect.bottom())))
        }))
    }
}

impl LayoutOutput {
    pub(crate) fn point_in_transformed_box(&self, node: &NodeRef, x: f32, y: f32) -> bool {
        let Some(raw) = self.node_bounds.get(&node.id()) else {
            return false;
        };
        let Some(visual) = self.visual_rect(node) else {
            return false;
        };
        self.transformed
            .boxes
            .contains(
                node.id(),
                x - visual.x + raw.x,
                y - visual.y + raw.y,
                true,
                true,
            )
            .unwrap_or(true)
    }
}
