use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[derive(Default)]
struct PaintCountingMeasurer {
    glyph_requests: usize,
}

impl TextMeasurer for PaintCountingMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        FixedMeasurer.measure(text, font)
    }

    fn shape(&mut self, text: &str, font: &FontSpec) -> ShapedText {
        self.glyph_requests += 1;
        let (width, height) = self.measure(text, font);
        ShapedText {
            width,
            height,
            raster_run_id: self.glyph_requests as u64,
            ..ShapedText::default()
        }
    }
}

#[test]
fn intrinsic_probes_request_glyphs_only_for_final_painted_text() {
    for markup in [
        "<p style='width:130px'>wrapping text <span style='display:inline-block;padding:4px'>nested words</span> tail</p>",
        "<table style='width:250px'><tr><td>long first cell wraps over lines</td><td>second cell text</td></tr></table>",
        "<main style='display:grid;grid-template-columns:auto 1fr;width:250px'><section>first cell</section><section><p>more wrapping text in nested block</p></section></main>",
        "<main style='display:flex;width:220px'><section>first flexible item</section><section>second flexible item</section></main>",
        "<style>p::before{content:'prefix ';display:inline-block}</style><p>visible <span style='display:none'>hidden</span> text</p>",
    ] {
        let page = Page::parse(markup, "https://example.test/");
        let mut measurer = PaintCountingMeasurer::default();
        let painted = layout_page(&page, 800.0, 600.0, &mut measurer);
        let text_runs = painted.items.iter().filter(|item| {
            matches!(item, DisplayItem::Text { raster_run_id, .. } if *raster_run_id != 0)
        }).count();
        assert!(text_runs > 0, "{markup}");
        assert_eq!(
            measurer.glyph_requests, text_runs,
            "intrinsic glyph copies: {markup}"
        );
        measurer.glyph_requests = 0;
        // Exercise the internal no-paint path directly, without the external MetricsOnly adapter.
        let geometry = layout_page_for_output(&page, 800.0, 600.0, 800.0, &mut measurer, false);
        assert_eq!(measurer.glyph_requests, 0, "{markup}");
        assert_eq!(geometry.node_bounds, painted.node_bounds, "{markup}");
        assert_eq!(geometry.content_height, painted.content_height, "{markup}");
    }
}
