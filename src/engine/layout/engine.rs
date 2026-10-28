use super::*;
mod block_measure;
pub(super) mod box_tree;
mod canvas;
pub(crate) mod fixed_layers;
mod height_basis;
mod intrinsic_widths;
mod layout_reads;
mod measured;
pub(in crate::engine::layout) use layout_reads::LayoutReads;
mod popover;
mod post_layout_index;
pub(in crate::engine::layout) use post_layout_index::ComposedNodes;
pub(crate) use post_layout_index::PostLayoutIndex;
mod probe_cache;
mod stats;
pub use stats::LayoutStats;
pub(crate) mod reuse;
mod subtree_replay;
pub(in crate::engine::layout) mod width_cache;

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
        fixed_layers::OutputRequest::paint(),
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
        fn normal_line_height(&mut self, font: &FontSpec) -> f32 {
            self.0.normal_line_height(font)
        }
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            self.0.measure(text, font)
        }
        fn text_geometry(&mut self, text: &str, font: &FontSpec) -> TextGeometry {
            self.0.text_geometry(text, font)
        }
        fn layout_cache_epoch(&self) -> Option<u64> {
            self.0.layout_cache_epoch()
        }
    }
    layout_page_for_output(
        page,
        viewport_width,
        viewport_height,
        style_viewport_width,
        &mut MetricsOnly(measurer),
        fixed_layers::OutputRequest::geometry(),
    )
}

fn layout_page_for_output<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
    request: fixed_layers::OutputRequest,
) -> LayoutOutput {
    let mut clock = stats::PhaseClock::start();
    let mut pass = LayoutStats {
        passes: 1,
        ..LayoutStats::default()
    };
    let emit_paint = request.emit_paint;
    let partial = request.root.is_some();
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
    let mut root = request
        .root
        .clone()
        .or(fullscreen_root)
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
    clock.restart();
    let box_tree = box_tree::BoxTree::retained(&root, styles, &page.layout_box_tree, !partial);
    pass.box_tree_micros = clock.lap();
    page.layout_reuse
        .borrow_mut()
        .begin_pass(
            measurer
                .layout_cache_epoch()
                .map(|measurer_epoch| reuse::ReuseEnvironment {
                    viewport: (viewport_width, viewport_height),
                    style_viewport_width,
                    scrollbar: page.scrollbar_thickness(),
                    canvas_replaced: page.canvas_is_replaced(),
                    fullscreen: root.is_fullscreen().then(|| root.id()),
                    measurer_epoch,
                }),
        );
    clock.restart();
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
        reads: Default::default(),
        replayed_roots: HashMap::new(),
        absolute_scope: Default::default(),
        viewport: RectF {
            x: 0.0,
            y: 0.0,
            width: viewport_width.max(1.0),
            height: viewport_height.max(1.0),
        },
        output: LayoutOutput {
            transformed: Default::default(),
            fragments: Default::default(),
            sticky_offsets: HashMap::new(),
            sticky_layers: Vec::new(),
            scroll_boxes: HashMap::new(),
            clip_paths: HashMap::new(),
            hit_excluded: HashSet::new(),
            items: Vec::new(),
            content_height: viewport_height,
            background: Color::WHITE,
            forms: if emit_paint && !partial {
                collect_forms(page)
            } else {
                HashMap::new()
            },
            node_bounds: HashMap::new(),
            resize_boxes: HashMap::new(),
            node_paint_order: Vec::new(),
            fixed_layers: fixed_layers::FixedLayers::new(
                viewport_width,
                viewport_height,
                style_viewport_width,
            ),
        },
    };
    std::sync::Arc::make_mut(&mut engine.output.fragments).next_line = request.first_line;

    if root.is_fullscreen() {
        // A fullscreen element is painted in the top layer over the default black backdrop.
        // Selecting it as the layout root also excludes page siblings from display and hit testing.
        engine.output.background = Color::BLACK;
    }
    let canvas = (!root.is_fullscreen())
        .then(|| engine.begin_canvas_background(&root))
        .flatten();
    clock.restart();
    let metrics = engine.layout_block(
        &root,
        0.0,
        0.0,
        viewport_width.max(1.0),
        Some(viewport_height.max(1.0)),
        None,
    );
    pass.block_layout_micros = clock.lap();
    // Before paint-order finalization turns stacking markers into node entries.
    let emitted = engine.output.items.len() as u64;
    let composed = page.layout_index.borrow_mut().composed(&root, styles);
    inline_layout::geometry::finish(&composed.inlines(), &mut engine.output);
    pass.inline_geometry_micros = clock.lap();
    engine.output.content_height = metrics
        .bottom
        .max(engine.scrollable_overflow_bottom(&root))
        .max(viewport_height);
    if let Some(canvas) = canvas {
        engine.finish_canvas_background(canvas, &root);
    }
    pass.overflow_micros = clock.lap();
    block::paint_order::finalize(&mut engine.output);
    pass.paint_order_micros = clock.lap();
    if !partial {
        popover::append_top_layer(
            page,
            styles,
            &mut engine.output,
            viewport_width,
            viewport_height,
            &mut *engine.measurer,
            emit_paint,
        );
    }
    clock.restart();
    if emit_paint {
        // Text display items retain text-node IDs, so their inherited eligibility is resolved
        // alongside element IDs.
        let hit_root = if partial { &root } else { &page.dom.document };
        let excluded = page.layout_index.borrow_mut().excluded(hit_root, styles);
        engine.output.hit_excluded.extend(excluded.nodes());
    }
    pass.hit_test_micros = clock.lap();
    box_tree.remove_anonymous_geometry(&mut engine.output);
    clock.restart();
    let document = (!partial).then_some(&*composed);
    let (width, height) = (viewport_width, viewport_height);
    engine
        .output
        .place_sticky_boxes(page, styles, document, width, height);
    pass.sticky_micros = clock.lap();
    fixed_layers::retain_image_inputs(page, &mut engine.output);
    if !partial {
        page.layout_reuse.borrow_mut().finish_pass();
    }
    pass.boxes_rebuilt = box_tree.rebuilt as u64;
    pass.items_emitted = emitted;
    page.count_layout(|counted| *counted = counted.saturating_add(pass));
    let output = engine.output;
    box_tree.retain();
    output
}

pub(super) struct LayoutEngine<'a, M> {
    pub(super) page: &'a Page,
    pub(super) styles: &'a box_tree::BoxTree<'a>,
    pub(super) measurer: &'a mut M,
    pub(super) emit_paint: bool,
    pub(super) retain_fragments: bool,
    pub(super) scroll_gutters: HashMap<NodeId, (bool, bool)>,
    pub(super) measurement_cache: HashMap<(usize, bool, u32), CachedAtomMeasurement>,
    intrinsic_block_heights: HashMap<block_measure::MeasureKey, (f32, LayoutReads)>,
    pub(super) intrinsic_widths: intrinsic_widths::IntrinsicWidths,
    pub(super) margin_profiles:
        std::cell::RefCell<HashMap<(NodeId, u32), block::margins::MarginProfile>>,
    pub(super) inline_box_cache: HashMap<(usize, u32), InlineBoxMetrics>,
    pub(super) viewport: RectF,
    pub(super) output: LayoutOutput,
    pub(super) positioned_flow_scopes: Vec<Vec<InFlowPaintRange>>,
    pub(super) floats: block::floats::FloatContext,
    pub(super) reads: std::cell::RefCell<layout_reads::ReadFrames>,
    /// Subtree roots whose final placement in this output replays a retained entry, by serial.
    pub(super) replayed_roots: HashMap<NodeId, u64>,
    /// The containing block of absolutely positioned descendants.
    pub(super) absolute_scope: block::AbsoluteScope,
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
                && style.mask_image.0.is_empty();
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
