use super::*;

#[cfg(test)]
mod fullscreen;
#[cfg(test)]
mod geometry;

pub fn layout_page<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    measurer: &mut M,
) -> LayoutOutput {
    layout_page_with_style_viewport(
        page,
        viewport_width,
        viewport_height,
        viewport_width,
        measurer,
    )
}

/// Lays out a page when the CSS media viewport and content area have different widths.
/// Classic scrollbars occupy content space but remain part of the media-query viewport.
pub fn layout_page_with_style_viewport<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
) -> LayoutOutput {
    layout_page_for_output(
        page,
        viewport_width,
        viewport_height,
        style_viewport_width,
        measurer,
        true,
    )
}

/// Resolves the same element boxes as retained layout without constructing paint or form output.
/// CSSOM View needs sizing, inline placement, and transformed descendants, not a display list.
pub fn layout_geometry_with_style_viewport<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
) -> HashMap<NodeId, RectF> {
    // Preserve font metrics without requesting positioned/rasterized glyph payloads that the
    // geometry caller cannot consume. TextMeasurer::shape defaults to these same measurements.
    struct MetricsOnly<'a, M>(&'a mut M);
    impl<M: TextMeasurer> TextMeasurer for MetricsOnly<'_, M> {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            self.0.measure(text, font)
        }
    }
    layout_page_for_output(
        page,
        viewport_width,
        viewport_height,
        style_viewport_width,
        &mut MetricsOnly(measurer),
        false,
    )
    .node_bounds
}

fn layout_page_for_output<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
    emit_paint: bool,
) -> LayoutOutput {
    let computed_styles;
    let cached_styles = page.cached_style_for_viewport(style_viewport_width, viewport_height);
    let styles = if let Some(cached_styles) = cached_styles {
        cached_styles
    } else {
        computed_styles = page.style_for_viewport(style_viewport_width, viewport_height);
        &computed_styles
    };
    // Top-layer boxes are still suppressed by display:none ancestry (CSS Position 4 §3.1).
    let fullscreen_root = Node::shadow_including_descendants(&page.dom.document).find(|node| {
        node.is_fullscreen()
            && std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
                .filter(|node| node.element().is_some())
                .all(|node| {
                    styles
                        .styles
                        .get(&node.id())
                        .is_some_and(|style| style.display != Display::None)
                })
    });
    let mut root = fullscreen_root
        .or_else(|| page.dom.elements_named("body").next())
        .or_else(|| page.dom.elements_named("html").next())
        .unwrap_or_else(|| page.dom.document.clone());
    // A layout-only cache can stop at a display:none ancestor of the usual body root.
    if !root.is_fullscreen()
        && let Some(hidden) =
            std::iter::successors(Some(root.clone()), Node::composed_parent).find(|node| {
                styles
                    .styles
                    .get(&node.id())
                    .is_some_and(|style| style.display == Display::None)
            })
    {
        root = hidden;
    }
    while !styles.styles.contains_key(&root.id()) {
        let Some(parent) = Node::composed_parent(&root) else {
            break;
        };
        root = parent;
    }
    let mut engine = LayoutEngine {
        page,
        styles,
        measurer,
        emit_paint,
        measurement_cache: HashMap::new(),
        inline_box_cache: HashMap::new(),
        positioned_flow_scopes: Vec::new(),
        viewport: RectF {
            x: 0.0,
            y: 0.0,
            width: viewport_width.max(1.0),
            height: viewport_height.max(1.0),
        },
        output: LayoutOutput {
            items: Vec::new(),
            content_height: viewport_height,
            background: Color::WHITE,
            forms: if emit_paint {
                collect_forms(page)
            } else {
                HashMap::new()
            },
            node_bounds: HashMap::new(),
            node_paint_order: Vec::new(),
        },
    };

    if root.is_fullscreen() {
        // A fullscreen element is painted in the top layer over the default black backdrop.
        // Selecting it as the layout root also excludes page siblings from display and hit testing.
        engine.output.background = Color::BLACK;
    } else if let Some(body_style) = engine.styles.styles.get(&node_id(&root))
        && body_style.background_color.alpha > 0
    {
        engine.output.background = body_style.background_color.composite_over(Color::WHITE);
    }
    let metrics = engine.layout_block(
        &root,
        0.0,
        0.0,
        viewport_width.max(1.0),
        Some(viewport_height.max(1.0)),
        None,
    );
    if emit_paint {
        engine.output.content_height = metrics
            .bottom
            .max(engine.scrollable_overflow_bottom(&root))
            .max(viewport_height);
    }
    engine.output
}

pub(super) struct LayoutEngine<'a, M> {
    pub(super) page: &'a Page,
    pub(super) styles: &'a StyleSet,
    pub(super) measurer: &'a mut M,
    pub(super) emit_paint: bool,
    pub(super) measurement_cache: HashMap<(usize, bool, u32), CachedAtomMeasurement>,
    pub(super) inline_box_cache: HashMap<(usize, u32), InlineBoxMetrics>,
    pub(super) viewport: RectF,
    pub(super) output: LayoutOutput,
    pub(super) positioned_flow_scopes: Vec<Vec<InFlowPaintRange>>,
}

pub(super) struct InFlowPaintRange {
    pub(super) node: NodeId,
    pub(super) level: i32,
    pub(super) items: std::ops::Range<usize>,
    pub(super) nodes: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BlockMetrics {
    pub(super) bottom: f32,
}

#[derive(Clone, Copy)]
pub(super) struct UsedInlineSize {
    pub(super) outer: f32,
    pub(super) percentage_basis: f32,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Returns box-tree children, flattening `display: contents` wrappers such as Shadow DOM
    /// slots while preserving the assigned nodes' own block, flex, grid, or inline display.
    pub(super) fn box_children(&self, node: &NodeRef) -> Vec<NodeRef> {
        if self.styles.get(node).display == Display::None {
            return Vec::new();
        }
        fn append<M: TextMeasurer>(
            engine: &LayoutEngine<'_, M>,
            node: &NodeRef,
            output: &mut Vec<NodeRef>,
        ) {
            let mut children = Vec::new();
            if !node.is_generated_pseudo()
                && let Some(before) = engine.styles.generated_pseudo(node, PseudoElement::Before)
            {
                children.push(before);
            }
            children.extend(Node::composed_children(node));
            if !node.is_generated_pseudo()
                && let Some(after) = engine.styles.generated_pseudo(node, PseudoElement::After)
            {
                children.push(after);
            }
            for child in children {
                if child.element().is_some()
                    && engine.styles.get(&child).display == Display::Contents
                {
                    append(engine, &child, output);
                } else {
                    output.push(child);
                }
            }
        }

        let mut output = Vec::new();
        append(self, node, &mut output);
        output
    }

    /// Returns children participating in a block formatting context. A boxless inline wrapper
    /// around a block child is split by that block and therefore does not force the descendant
    /// subtree into one inline run (CSS 2.1 section 9.2.1.1).
    pub(super) fn block_formatting_children(&self, node: &NodeRef) -> Vec<NodeRef> {
        fn append<M: TextMeasurer>(
            engine: &LayoutEngine<'_, M>,
            child: NodeRef,
            output: &mut Vec<NodeRef>,
        ) {
            let style = engine.styles.get(&child);
            let children = engine.box_children(&child);
            let boxless_inline = child.element().is_some()
                && child.tag_name() != Some("a")
                && style.display == Display::Inline
                && style.margin.left == Length::Px(0.0)
                && style.margin.right == Length::Px(0.0)
                && style.padding == Edges::ZERO
                && style.border_width == Edges::ZERO
                && style.background_color.alpha == 0
                && style.background_image.is_none()
                && style.mask_image.is_none();
            if boxless_inline
                && children
                    .iter()
                    .any(|descendant| is_block_level(engine.styles.get(descendant).display))
            {
                for descendant in children {
                    append(engine, descendant, output);
                }
            } else {
                output.push(child);
            }
        }

        let mut output = Vec::new();
        for child in self.box_children(node) {
            append(self, child, &mut output);
        }
        output
    }
}
