//! CSS Backgrounds 3 §2.11.2: the canvas background.
//!
//! The root element's background, or the `<body>` background when the root's is transparent
//! (no color and no images), paints the whole canvas. Image layers are positioned against
//! the root element's padding box but clip only to the canvas, and the element whose
//! background was propagated paints none of its own.

use super::*;

/// Display-list slots reserved for the canvas image layers until the root box is known.
pub(super) struct CanvasBackground {
    source: NodeRef,
    slots: Vec<(usize, usize)>,
}

fn has_background(style: &ComputedStyle) -> bool {
    style.background_color.alpha > 0 || !style.background_image.is_none()
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// The element whose background is the canvas background, if any.
    fn canvas_background_source(&self, root: &NodeRef) -> Option<NodeRef> {
        if !root
            .parent()
            .is_some_and(|parent| matches!(parent.data, NodeData::Document))
        {
            return None;
        }
        let style = self.styles.styles.get(&root.id())?;
        if has_background(style) {
            return Some(root.clone());
        }
        if root.tag_name() != Some("html") {
            return None;
        }
        // HTML §3.1.3 "the body element": the first body child of the html element.
        let body = root
            .children
            .borrow()
            .iter()
            .find(|child| child.tag_name() == Some("body"))
            .cloned()?;
        self.styles
            .styles
            .get(&body.id())
            .is_some_and(|style| has_background(style))
            .then_some(body)
    }

    /// Whether this element's background was propagated to the canvas instead.
    pub(in crate::engine::layout) fn paints_canvas_background(&self, node: &NodeRef) -> bool {
        match node.tag_name() {
            Some("body") => node.parent().is_some_and(|root| {
                self.canvas_background_source(&root)
                    .is_some_and(|source| source.id() == node.id())
            }),
            _ => self
                .canvas_background_source(node)
                .is_some_and(|source| source.id() == node.id()),
        }
    }

    /// Sets the canvas color and reserves the image layers, which paint below everything.
    pub(super) fn begin_canvas_background(&mut self, root: &NodeRef) -> Option<CanvasBackground> {
        let source = self.canvas_background_source(root)?;
        let style = self.styles.get(&source).clone();
        if style.background_color.alpha > 0 {
            self.output.background = style.background_color.composite_over(Color::WHITE);
        }
        if !self.emit_paint || !style.visibility {
            return None;
        }
        // First specified image is nearest the user.
        let slots = (style.background_image.0.iter().enumerate().rev())
            .filter(|(_, image)| !matches!(image, ImageSource::None))
            .map(|(layer, _)| {
                self.output.items.push(DisplayItem::SolidRect {
                    rect: RectF::default(),
                    color: Color::TRANSPARENT,
                    radius: 0.0,
                });
                (self.output.items.len() - 1, layer)
            })
            .collect();
        Some(CanvasBackground { source, slots })
    }

    /// Fills the reserved layers once the root box and the canvas extent are known.
    pub(super) fn finish_canvas_background(&mut self, canvas: CanvasBackground, root: &NodeRef) {
        let Some(root_box) = self.output.node_bounds.get(&root.id()).copied() else {
            return;
        };
        let root_style = self.styles.get(root).clone();
        let style = self.styles.get(&canvas.source).clone();
        let border = root_style
            .border_width
            .resolve(root_box.width, root_style.font_size);
        let area = RectF {
            x: root_box.x + border.left,
            y: root_box.y + border.top,
            width: (root_box.width - border.horizontal()).max(0.0),
            height: (root_box.height - border.vertical()).max(0.0),
        };
        let canvas_rect = RectF {
            x: 0.0,
            y: 0.0,
            width: self.viewport.width.max(root_box.x + root_box.width),
            height: self.output.content_height,
        };
        for (index, layer) in canvas.slots {
            let image = &style.background_image.0[layer];
            if let Some(item) =
                self.image_layer_item(&style, image, layer, canvas_rect, area, 0.0, false)
            {
                self.output.items[index] = item;
            }
        }
    }
}
