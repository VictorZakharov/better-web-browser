//! Intrinsic contributions are stable within a layout pass, including its sizing probes.
//! A new pass gets a fresh cache so DOM, style, font and image changes cannot leave stale sizes.
use super::*;

#[derive(Default)]
pub(in crate::engine::layout) struct IntrinsicWidths {
    entries: HashMap<(NodeId, Option<u32>, bool), (f32, f32)>,
    max_content: HashMap<(NodeId, Option<u32>), f32>,
}

impl IntrinsicWidths {
    pub(in crate::engine::layout) fn get(
        &self,
        node: NodeId,
        basis: impl Into<Option<f32>>,
        outer: bool,
    ) -> Option<(f32, f32)> {
        self.entries
            .get(&(node, basis.into().map(f32::to_bits), outer))
            .copied()
    }

    pub(in crate::engine::layout) fn insert(
        &mut self,
        node: NodeId,
        basis: impl Into<Option<f32>>,
        outer: bool,
        widths: (f32, f32),
    ) {
        // Wide/deep documents may probe many percentage bases; overflow just recomputes.
        if self.entries.len() + self.max_content.len() < crate::limits::MAX_DOM_NODES {
            self.entries
                .insert((node, basis.into().map(f32::to_bits), outer), widths);
        }
    }

    pub(in crate::engine::layout) fn max_content(
        &self,
        node: NodeId,
        basis: Option<f32>,
    ) -> Option<f32> {
        self.max_content
            .get(&(node, basis.map(f32::to_bits)))
            .copied()
    }

    pub(in crate::engine::layout) fn insert_max_content(
        &mut self,
        node: NodeId,
        basis: Option<f32>,
        width: f32,
    ) {
        // The node's computed display selects block/flex sizing and is stable for this pass.
        // Indefinite and definite-zero bases differ for cyclic percentage constraints.
        if self.entries.len() + self.max_content.len() < crate::limits::MAX_DOM_NODES {
            self.max_content
                .insert((node, basis.map(f32::to_bits)), width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::CountingMeasurer;

    #[test]
    fn keys_preserve_node_percentage_basis_and_contribution_kind() {
        let page = Page::parse("<div></div><span></span>", "https://example.test/");
        let div = page.dom.elements_named("div").next().unwrap();
        let span = page.dom.elements_named("span").next().unwrap();
        let mut cache = IntrinsicWidths::default();
        cache.insert(div.id(), 200.0, false, (20.0, 80.0));
        assert_eq!(cache.get(div.id(), 200.0, false), Some((20.0, 80.0)));
        assert_eq!(cache.get(div.id(), 200.001, false), None);
        assert_eq!(cache.get(div.id(), 400.0, false), None);
        assert_eq!(cache.get(div.id(), 200.0, true), None);
        assert_eq!(cache.get(span.id(), 200.0, false), None);
        cache.insert(div.id(), None, false, (10.0, 100.0));
        assert_eq!(cache.get(div.id(), None, false), Some((10.0, 100.0)));
        assert_eq!(cache.get(div.id(), 0.0, false), None);
        cache.insert_max_content(div.id(), None, 100.0);
        assert_eq!(cache.max_content(div.id(), None), Some(100.0));
        assert_eq!(cache.max_content(div.id(), Some(0.0)), None);
        assert_eq!(cache.max_content(span.id(), None), None);
    }

    #[test]
    fn repeated_contributions_reuse_measurements_and_match_cold_results() {
        let page = Page::parse(
            "<style>main{display:flex} main::before{content:'prefix'} span{display:inline-block;padding:2%}</style><main><p>long words <span>nested words</span></p><img width=40 height=20></main>",
            "https://example.test/",
        );
        let node = page.dom.elements_named("main").next().unwrap();
        let styles = page.style(800.0);
        let mut measurer = CountingMeasurer::default();
        let mut engine = LayoutEngine {
            page: &page,
            styles: &styles,
            measurer: &mut measurer,
            emit_paint: false,
            scroll_gutters: HashMap::new(),
            measurement_cache: HashMap::new(),
            intrinsic_block_heights: HashMap::new(),
            intrinsic_widths: Default::default(),
            margin_profiles: Default::default(),
            inline_box_cache: HashMap::new(),
            viewport: RectF {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            output: LayoutOutput::default(),
            positioned_flow_scopes: Vec::new(),
            floats: Default::default(),
        };
        for basis in [200.0, 400.0, 200.001] {
            let widths = engine.float_intrinsic_widths(&node, basis);
            let calls = engine.measurer.calls;
            assert!(calls > 0);
            assert_eq!(engine.float_intrinsic_widths(&node, basis), widths);
            assert_eq!(engine.measurer.calls, calls, "a hit must not measure text");
            engine.intrinsic_widths = Default::default();
            assert_eq!(engine.float_intrinsic_widths(&node, basis), widths);
            assert!(engine.measurer.calls > calls, "cold results are recomputed");
        }
        for tag in ["main", "p"] {
            let item = page.dom.elements_named(tag).next().unwrap();
            let style = styles.get(&item).clone();
            let width = engine.flex_item_basis(&item, &style, 200.0);
            let calls = engine.measurer.calls;
            assert_eq!(engine.flex_item_basis(&item, &style, 200.0), width);
            assert_eq!(engine.measurer.calls, calls, "cached {tag} max-content");
            engine.intrinsic_widths = Default::default();
            assert_eq!(engine.flex_item_basis(&item, &style, 200.0), width);
            assert!(engine.measurer.calls > calls, "cold {tag} max-content");
        }
        let widths = engine.float_intrinsic_widths(&node, 200.0);
        engine.intrinsic_block_height(&node, 200.0, None, None);
        assert_eq!(
            engine.intrinsic_widths.get(node.id(), 200.0, true),
            Some(widths),
            "a no-paint height probe returns the shared width cache"
        );
    }

    #[test]
    fn later_layout_passes_observe_style_changes() {
        let page = Page::parse(
            "<style>body{margin:0} aside{float:left} p{margin:0}</style><aside><p>words</p></aside>",
            "https://example.test/",
        );
        let aside = page.dom.elements_named("aside").next().unwrap();
        let paragraph = page.dom.elements_named("p").next().unwrap();
        let before = layout_page(&page, 800.0, 600.0, &mut CountingMeasurer::default());
        paragraph.set_attr("style", "font-size:40px");
        let after = layout_page(&page, 800.0, 600.0, &mut CountingMeasurer::default());
        assert!(after.node_bounds[&aside.id()].width > before.node_bounds[&aside.id()].width);
    }
}
