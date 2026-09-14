//! Retained sticky constraints are evaluated at the current scroll offset, without layout.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct StickyLayer {
    pub node_id: NodeId,
    pub items: std::ops::Range<usize>,
    pub normal: RectF,
    pub containing: RectF,
    pub port: RectF,
    /// Top, right, bottom, left, resolved against the nearest scrollport.
    pub insets: [Option<f32>; 4],
    pub margins: [f32; 4],
    pub parent: Option<usize>,
    pub port_parent: Option<usize>,
    pub viewport_port: bool,
    /// The translation already applied to this layer's retained display items.
    pub offset: (f32, f32),
}

impl LayoutOutput {
    /// Reconcile even an older renderer presentation with the browser's latest viewport.
    /// CSS Position 3 makes sticky displacement a visual adjustment, not normal-flow layout.
    pub fn update_scroll_position(&mut self, x: f32, y: f32) -> bool {
        let mut accumulated = Vec::<(f32, f32)>::with_capacity(self.sticky_layers.len());
        let mut changed = false;
        for layer in &mut self.sticky_layers {
            let parent = layer.parent.map_or((0.0, 0.0), |i| accumulated[i]);
            let port = if layer.viewport_port {
                (x, y)
            } else {
                layer.port_parent.map_or((0.0, 0.0), |i| accumulated[i])
            };
            let next = (
                sticky_axis(
                    layer.normal.x + parent.0,
                    layer.normal.width,
                    layer.containing.x + parent.0,
                    layer.containing.width,
                    layer.port.x + port.0,
                    layer.port.width,
                    layer.insets[3],
                    layer.insets[1],
                    layer.margins[3],
                    layer.margins[1],
                ),
                sticky_axis(
                    layer.normal.y + parent.1,
                    layer.normal.height,
                    layer.containing.y + parent.1,
                    layer.containing.height,
                    layer.port.y + port.1,
                    layer.port.height,
                    layer.insets[0],
                    layer.insets[2],
                    layer.margins[0],
                    layer.margins[2],
                ),
            );
            let delta = (next.0 - layer.offset.0, next.1 - layer.offset.1);
            if delta != (0.0, 0.0) {
                translate::translate_display_items(
                    &mut self.items[layer.items.clone()],
                    delta.0,
                    delta.1,
                );
                changed = true;
            }
            layer.offset = next;
            self.sticky_offsets.insert(layer.node_id, next);
            accumulated.push((parent.0 + next.0, parent.1 + next.1));
        }
        changed
    }
}
