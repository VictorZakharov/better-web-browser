use super::*;

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
    let computed_styles;
    let cached_styles = page.cached_style_for_viewport(style_viewport_width, viewport_height);
    let styles = if let Some(cached_styles) = cached_styles {
        cached_styles
    } else {
        computed_styles = page.style_for_viewport(style_viewport_width, viewport_height);
        &computed_styles
    };
    let mut engine = LayoutEngine {
        page,
        styles,
        measurer,
        measurement_cache: HashMap::new(),
        inline_box_cache: HashMap::new(),
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
            forms: collect_forms(page),
        },
    };

    let root = page
        .dom
        .elements_named("body")
        .next()
        .or_else(|| page.dom.elements_named("html").next())
        .unwrap_or_else(|| page.dom.document.clone());
    if let Some(body_style) = engine.styles.styles.get(&node_id(&root))
        && body_style.background_color.alpha > 0
    {
        engine.output.background = body_style.background_color.composite_over(Color::WHITE);
    }
    let metrics = engine.layout_block(&root, 0.0, 0.0, viewport_width.max(1.0));
    engine.output.content_height = metrics.bottom.max(viewport_height);
    engine.output
}

pub(super) struct LayoutEngine<'a, M> {
    pub(super) page: &'a Page,
    pub(super) styles: &'a StyleSet,
    pub(super) measurer: &'a mut M,
    pub(super) measurement_cache: HashMap<(usize, bool, u32), CachedAtomMeasurement>,
    pub(super) inline_box_cache: HashMap<(usize, u32), InlineBoxMetrics>,
    pub(super) viewport: RectF,
    pub(super) output: LayoutOutput,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BlockMetrics {
    pub(super) bottom: f32,
}
