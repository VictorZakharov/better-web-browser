//! Compose same-process child browsing contexts into their iframe paint boxes.
//!
//! An iframe is a replaced element in its parent, but its child document has its
//! own viewport, style cascade, and event target tree. Keep those coordinates and
//! identities separate until the renderer builds a presentation.

use super::*;
use crate::engine::css::Color;
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::layout::translate_display_items;
use crate::engine::script::runtime::FramePaintSnapshot;
use crate::engine::{DisplayItem, LayoutOutput, RectF};

pub(super) struct PaintedFrame {
    pub document: NodeId,
    pub rect: RectF,
    pub page: Page,
    pub layout: LayoutOutput,
    pub children: Vec<PaintedFrame>,
}

pub(super) fn append_images(
    frames: &[PaintedFrame],
    sent: &mut HashSet<String>,
    presented: &mut Vec<PresentedImage>,
) {
    for frame in frames {
        for (url, image) in &frame.page.images {
            if url.len() <= crate::limits::MAX_URL_BYTES && sent.insert(url.clone()) {
                presented.push(PresentedImage {
                    url: url.clone(),
                    image: image.clone(),
                });
            }
        }
        append_images(&frame.children, sent, presented);
    }
}

pub(super) fn viewports(frames: &[PaintedFrame], output: &mut Vec<(NodeId, RectF)>) {
    for frame in frames {
        output.push((frame.document, frame.rect));
        viewports(&frame.children, output);
    }
}

impl DocumentRuntime {
    pub(super) fn compose_embedded_frames(&mut self) {
        self.frame_paint.clear();
        let Some(runtime) = self.script_runtime.as_mut() else {
            return;
        };
        let snapshots = runtime.frame_paint_snapshots();
        if snapshots.is_empty() {
            return;
        }
        let mut text = self.text.borrow_mut();
        let mut frames = Vec::new();
        let original = std::mem::take(&mut self.layout.items);
        let mut boundaries = Vec::new();
        self.layout.items =
            compose_items(original, snapshots, &mut frames, &mut text, &mut boundaries);
        remap_sticky_layers(&mut self.layout, &boundaries);
        self.frame_paint = frames;
    }
}

fn compose_items(
    items: Vec<DisplayItem>,
    snapshots: Vec<FramePaintSnapshot>,
    frames: &mut Vec<PaintedFrame>,
    text: &mut RendererTextSystem,
    boundaries: &mut Vec<usize>,
) -> Vec<DisplayItem> {
    let mut snapshots: HashMap<NodeId, FramePaintSnapshot> = snapshots
        .into_iter()
        .map(|frame| (frame.element, frame))
        .collect();
    let mut composed = Vec::with_capacity(items.len());
    for item in items {
        boundaries.push(composed.len());
        let DisplayItem::EmbeddedFrame { rect, node_id } = item else {
            composed.push(item);
            continue;
        };
        if rect.width <= 0.0 || rect.height <= 0.0 {
            continue;
        }
        let Some(snapshot) = snapshots.remove(&node_id) else {
            continue;
        };
        let mut page = Page::from_frame_document(
            snapshot.dom,
            &snapshot.url,
            snapshot.stylesheets,
            snapshot.quirks_mode,
            snapshot.media_environment,
        );
        page.images.extend(snapshot.images);
        page.set_layout_viewport(rect.width, rect.height);
        let mut layout =
            layout_page_with_style_viewport(&page, rect.width, rect.height, rect.width, text);
        (snapshot.publish_geometry)(&layout, rect);
        let mut children = Vec::new();
        let mut child_boundaries = Vec::new();
        layout.items = compose_items(
            layout.items,
            snapshot.children,
            &mut children,
            text,
            &mut child_boundaries,
        );
        remap_sticky_layers(&mut layout, &child_boundaries);
        let mut paint = layout.items.clone();
        translate_display_items(&mut paint, rect.x, rect.y);
        composed.push(DisplayItem::BeginClip { bounds: rect });
        if layout.background.alpha > 0 {
            composed.push(DisplayItem::SolidRect {
                rect,
                color: layout.background,
                radius: 0.0,
            });
        } else {
            composed.push(DisplayItem::SolidRect {
                rect,
                color: Color::rgb(255, 255, 255),
                radius: 0.0,
            });
        }
        composed.extend(paint);
        composed.push(DisplayItem::EndClip { bounds: rect });
        frames.push(PaintedFrame {
            document: snapshot.document,
            rect,
            page,
            layout,
            children,
        });
    }
    boundaries.push(composed.len());
    composed
}

fn remap_sticky_layers(layout: &mut LayoutOutput, boundaries: &[usize]) {
    for layer in &mut layout.sticky_layers {
        let start = boundaries.get(layer.items.start).copied().unwrap_or(0);
        let end = boundaries.get(layer.items.end).copied().unwrap_or(start);
        layer.items = start..end;
    }
}

pub(super) fn hit_frame(
    frames: &[PaintedFrame],
    x: f32,
    y: f32,
) -> Option<(&PaintedFrame, Option<NodeRef>, f32, f32)> {
    for frame in frames.iter().rev() {
        if !contains(frame.rect, x, y) {
            continue;
        }
        let x = x - frame.rect.x;
        let y = y - frame.rect.y;
        if let Some(child) = hit_frame(&frame.children, x, y) {
            return Some(child);
        }
        let target = frame.layout.node_paint_order.iter().rev().find_map(|id| {
            let node = frame.page.dom.find_node(*id)?;
            let rect = frame.layout.visual_rect(&node)?;
            (contains(rect, x, y)
                && !frame.layout.hit_excluded.contains(id)
                && frame.layout.point_in_scroll_clips(&node, x, y))
            .then_some(node)
        });
        return Some((frame, target, x, y));
    }
    None
}

fn contains(rect: RectF, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;
    use crate::engine::layout::StickyLayer;

    #[test]
    fn child_document_paints_in_its_own_viewport_and_receives_local_hits() {
        let child = dom::parse(
            "<style>body{margin:0}</style><button style='width:100px;height:40px'>child</button>",
        );
        let document = child.document.id();
        let element = dom::parse("<iframe></iframe>")
            .elements_named("iframe")
            .next()
            .unwrap()
            .id();
        let rect = RectF {
            x: 20.0,
            y: 30.0,
            width: 304.0,
            height: 78.0,
        };
        let snapshot = FramePaintSnapshot {
            element,
            document,
            dom: child.document,
            url: "about:blank".into(),
            stylesheets: Vec::new(),
            media_environment: MediaEnvironment::new(304.0, 78.0, 1.0, false),
            quirks_mode: false,
            images: HashMap::new(),
            children: Vec::new(),
            publish_geometry: Box::new(|_, _| {}),
        };
        let mut frames = Vec::new();
        let mut text = RendererTextSystem::new(96);
        let items = compose_items(
            vec![DisplayItem::EmbeddedFrame {
                rect,
                node_id: element,
            }],
            vec![snapshot],
            &mut frames,
            &mut text,
            &mut Vec::new(),
        );
        assert!(
            matches!(items.first(), Some(DisplayItem::BeginClip { bounds }) if *bounds == rect)
        );
        assert!(items.iter().any(|item| {
            matches!(item, DisplayItem::Text { text, rect, .. }
                if text.contains("child") && rect.x >= 20.0 && rect.y >= 30.0)
        }));
        let (frame, target, x, y) = hit_frame(&frames, 50.0, 45.0).unwrap();
        assert_eq!(frame.document, document);
        assert_eq!((x, y), (30.0, 15.0));
        assert!(target.is_some());
    }

    #[test]
    fn embedded_paint_preserves_parent_sticky_item_ranges() {
        let mut layout = LayoutOutput::default();
        let zero = RectF {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        layout.sticky_layers.push(StickyLayer {
            node_id: dom::parse("<div></div>").document.id(),
            items: 1..2,
            normal: zero,
            containing: zero,
            port: zero,
            insets: [None; 4],
            margins: [0.0; 4],
            parent: None,
            port_parent: None,
            viewport_port: true,
            offset: (0.0, 0.0),
        });
        remap_sticky_layers(&mut layout, &[0, 1, 7, 8]);
        assert_eq!(layout.sticky_layers[0].items, 1..7);
    }
}
