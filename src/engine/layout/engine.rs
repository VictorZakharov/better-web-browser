use super::*;
mod block_measure;
pub(super) mod box_tree;
mod intrinsic_widths;

#[cfg(test)]
mod fullscreen;
#[cfg(test)]
mod geometry;
#[cfg(test)]
mod geometry_glyphs;

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
/// CSSOM View needs sizing, inline placement, transformed descendants, and scrollable extent.
/// The returned paint, form, and paint-order collections remain empty.
pub fn layout_geometry_with_style_viewport<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
) -> LayoutOutput {
    // Preserve font metrics without requesting positioned/rasterized glyph payloads that the
    // geometry caller cannot consume. TextMeasurer::shape defaults to these same measurements.
    struct MetricsOnly<'a, M>(&'a mut M);
    impl<M: TextMeasurer> TextMeasurer for MetricsOnly<'_, M> {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            self.0.measure(text, font)
        }
        fn text_geometry(&mut self, text: &str, font: &FontSpec) -> TextGeometry {
            self.0.text_geometry(text, font)
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
        .or_else(|| page.dom.elements_named("html").next())
        .or_else(|| page.dom.elements_named("body").next())
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
    let box_tree = box_tree::BoxTree::new(&root, styles);
    let mut engine = LayoutEngine {
        page,
        styles: &box_tree,
        measurer,
        emit_paint,
        retain_fragments: true,
        scroll_gutters: HashMap::new(),
        measurement_cache: HashMap::new(),
        intrinsic_block_heights: HashMap::new(),
        intrinsic_widths: Default::default(),
        margin_profiles: Default::default(),
        inline_box_cache: HashMap::new(),
        positioned_flow_scopes: Vec::new(),
        floats: Default::default(),
        viewport: RectF {
            x: 0.0,
            y: 0.0,
            width: viewport_width.max(1.0),
            height: viewport_height.max(1.0),
        },
        output: LayoutOutput {
            fragments: Default::default(),
            sticky_offsets: HashMap::new(),
            sticky_layers: Vec::new(),
            scroll_boxes: HashMap::new(),
            clip_paths: HashMap::new(),
            hit_excluded: HashSet::new(),
            items: Vec::new(),
            content_height: viewport_height,
            background: Color::WHITE,
            forms: if emit_paint {
                collect_forms(page)
            } else {
                HashMap::new()
            },
            node_bounds: HashMap::new(),
            resize_boxes: HashMap::new(),
            node_paint_order: Vec::new(),
        },
    };

    if root.is_fullscreen() {
        // A fullscreen element is painted in the top layer over the default black backdrop.
        // Selecting it as the layout root also excludes page siblings from display and hit testing.
        engine.output.background = Color::BLACK;
    } else {
        let backdrop = engine
            .styles
            .styles
            .get(&node_id(&root))
            .filter(|style| style.background_color.alpha > 0)
            .or_else(|| {
                page.dom.elements_named("body").next().and_then(|body| {
                    engine
                        .styles
                        .styles
                        .get(&node_id(&body))
                        .filter(|style| style.background_color.alpha > 0)
                })
            });
        if let Some(style) = backdrop {
            engine.output.background = style.background_color.composite_over(Color::WHITE);
        }
    }
    let metrics = engine.layout_block(
        &root,
        0.0,
        0.0,
        viewport_width.max(1.0),
        Some(viewport_height.max(1.0)),
        None,
    );
    inline_layout::geometry::finish(&root, styles, &mut engine.output);
    engine.output.content_height = metrics
        .bottom
        .max(engine.scrollable_overflow_bottom(&root))
        .max(viewport_height);
    block::paint_order::finalize(&mut engine.output);
    if emit_paint {
        // Text display items retain text-node IDs, so resolve their inherited
        // eligibility alongside element IDs in one ancestor-first DOM walk.
        for node in Node::shadow_including_descendants(&page.dom.document) {
            let eligible = styles
                .styles
                .get(&node.id())
                .map(|style| style.pointer_events && style.visibility)
                .or_else(|| {
                    Node::composed_parent(&node)
                        .map(|parent| !engine.output.hit_excluded.contains(&parent.id()))
                })
                .unwrap_or(true);
            if !eligible {
                engine.output.hit_excluded.insert(node.id());
            }
        }
    }
    box_tree.remove_anonymous_geometry(&mut engine.output);
    engine.output.update_sticky_positions(
        page,
        viewport_width,
        viewport_height,
        style_viewport_width,
    );
    engine.output
}

pub(super) struct LayoutEngine<'a, M> {
    pub(super) page: &'a Page,
    pub(super) styles: &'a box_tree::BoxTree<'a>,
    pub(super) measurer: &'a mut M,
    pub(super) emit_paint: bool,
    pub(super) retain_fragments: bool,
    pub(super) scroll_gutters: HashMap<NodeId, (bool, bool)>,
    pub(super) measurement_cache: HashMap<(usize, bool, u32), CachedAtomMeasurement>,
    intrinsic_block_heights: HashMap<block_measure::MeasureKey, f32>,
    pub(super) intrinsic_widths: intrinsic_widths::IntrinsicWidths,
    pub(super) margin_profiles:
        std::cell::RefCell<HashMap<(NodeId, u32), block::margins::MarginProfile>>,
    pub(super) inline_box_cache: HashMap<(usize, u32), InlineBoxMetrics>,
    pub(super) viewport: RectF,
    pub(super) output: LayoutOutput,
    pub(super) positioned_flow_scopes: Vec<Vec<InFlowPaintRange>>,
    pub(super) floats: block::floats::FloatContext,
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
        // Native select contents belong to its picker, not the surrounding box tree,
        // even when CSS blockifies the control (HTML rendering: the select element).
        if self.styles.get(node).display == Display::None || node.tag_name() == Some("select") {
            return Vec::new();
        }
        self.styles.children(node)
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
