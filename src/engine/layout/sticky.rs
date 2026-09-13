//! Sticky positioning adjusts visual placement, never normal-flow or offset geometry.
//! https://drafts.csswg.org/css-position-3/#sticky-pos
use super::*;
mod composition;
pub use composition::StickyLayer;
#[cfg(test)]
mod tests;

impl LayoutOutput {
    pub(crate) fn update_sticky_positions(
        &mut self,
        page: &Page,
        width: f32,
        height: f32,
        style_width: f32,
    ) {
        if self.sticky_layers.is_empty() {
            self.build_sticky_layers(page, width, height, style_width);
        }
        let (x, y) = page.dom.document.scroll_offset.get();
        self.update_scroll_position(x, y);
    }

    fn build_sticky_layers(&mut self, page: &Page, width: f32, height: f32, style_width: f32) {
        let computed;
        let styles = if let Some(styles) = page.cached_style_for_viewport(style_width, height) {
            styles
        } else {
            computed = page.style_for_viewport(style_width, height);
            &computed
        };
        let mut ranges = HashMap::new();
        {
            let mut starts = HashMap::new();
            for (index, item) in self.items.iter().enumerate() {
                if let DisplayItem::NodeBoundary { node_id, entering } = item {
                    if *entering {
                        starts.insert(*node_id, index);
                    } else if let Some(start) = starts.remove(node_id) {
                        ranges.insert(*node_id, start..index + 1);
                    }
                }
            }
        }
        let viewport = RectF {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let mut indices = HashMap::new();
        for node in Node::composed_descendants(&page.dom.document) {
            // Sparse layout deliberately omits styles below hidden subtrees.
            if !self.node_bounds.contains_key(&node.id()) {
                continue;
            }
            let style = styles.get(&node);
            if style.position != Position::Sticky {
                continue;
            }
            let Some(normal) = self.visual_rect(&node) else {
                continue;
            };
            let parent = std::iter::successors(Node::composed_parent(&node), Node::composed_parent)
                .find(|parent| self.node_bounds.contains_key(&parent.id()));
            let Some(parent) = parent else {
                continue;
            };
            let Some(mut containing) = self.visual_rect(&parent) else {
                continue;
            };
            let parent_style = styles.get(&parent);
            let sticky_parent = std::iter::successors(Some(parent.clone()), Node::composed_parent)
                .find_map(|ancestor| indices.get(&ancestor.id()).copied());
            let padding = parent_style
                .padding
                .resolve(containing.width, parent_style.font_size);
            let border = parent_style
                .border_width
                .resolve(containing.width, parent_style.font_size);
            containing.x += padding.left + border.left;
            containing.y += padding.top + border.top;
            containing.width =
                (containing.width - padding.horizontal() - border.horizontal()).max(0.0);
            containing.height =
                (containing.height - padding.vertical() - border.vertical()).max(0.0);
            if let Some(scroll) = self.scroll_boxes.get(&parent.id()) {
                // A scroll container's content containing block includes its scrolling
                // area; constraining to the visible padding box would unstick direct
                // children as soon as the first viewport of content scrolls away.
                containing.width = containing
                    .width
                    .max(scroll.content_width - padding.horizontal());
                containing.height = containing
                    .height
                    .max(scroll.content_height - padding.vertical());
                containing.x -= scroll.offset_x;
                containing.y -= scroll.offset_y;
            }
            let mut port = viewport;
            let mut viewport_port = true;
            let mut port_parent = None;
            for ancestor in std::iter::successors(Some(parent), Node::composed_parent) {
                if let Some(scroll) = self.scroll_boxes.get(&ancestor.id())
                    && (scroll.scroll_x || scroll.scroll_y)
                {
                    let raw = self.node_bounds[&ancestor.id()];
                    let visual = self.visual_rect(&ancestor).unwrap_or(raw);
                    port = RectF {
                        x: scroll.port.x + visual.x - raw.x,
                        y: scroll.port.y + visual.y - raw.y,
                        ..scroll.port
                    };
                    viewport_port = false;
                    port_parent = std::iter::successors(Some(ancestor), Node::composed_parent)
                        .find_map(|ancestor| indices.get(&ancestor.id()).copied());
                    break;
                }
            }
            let margins = style.margin.resolve(containing.width, style.font_size);
            indices.insert(node.id(), self.sticky_layers.len());
            self.sticky_layers.push(StickyLayer {
                node_id: node.id(),
                items: ranges.remove(&node.id()).unwrap_or(0..0),
                normal,
                containing,
                port,
                insets: [
                    style.top.resolve(port.height, style.font_size),
                    style.right.resolve(port.width, style.font_size),
                    style.bottom.resolve(port.height, style.font_size),
                    style.left.resolve(port.width, style.font_size),
                ],
                margins: [margins.top, margins.right, margins.bottom, margins.left],
                parent: sticky_parent,
                port_parent,
                viewport_port,
                offset: (0.0, 0.0),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sticky_axis(
    position: f32,
    size: f32,
    cb: f32,
    cb_size: f32,
    port: f32,
    port_size: f32,
    start: Option<f32>,
    end: Option<f32>,
    margin_start: f32,
    margin_end: f32,
) -> f32 {
    if start.is_none() && end.is_none() {
        return 0.0;
    }
    let view_start = port + start.unwrap_or(0.0);
    let view_end = (port + port_size - end.unwrap_or(0.0)).max(view_start + size);
    let mut target = position;
    if start.is_some() {
        target = target.max(view_start);
    }
    if end.is_some() {
        target = target.min(view_end - size);
    }
    if target == position {
        return 0.0;
    }
    let before = margin_start.min((position - cb).max(0.0));
    let after = margin_end.min((cb + cb_size - position - size).max(0.0));
    let lower = cb + before;
    let upper = (cb + cb_size - size - after).max(lower);
    target.clamp(lower, upper) - position
}
