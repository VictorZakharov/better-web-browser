use super::*;
struct FontMetricsMeasurer;
impl TextMeasurer for FontMetricsMeasurer {
    fn normal_line_height(&mut self, font: &FontSpec) -> f32 {
        font.size * 1.5
    }
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size / 2.0, font.size * 1.5)
    }
}

fn height(css: &str) -> f32 {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}div{{font:20px Arial;{css}}}</style><div>Agjp</div>"),
        "https://example.test/",
    );
    let node = page.dom.elements_named("div").next().unwrap();
    layout_page(&page, 800.0, 600.0, &mut FontMetricsMeasurer).node_bounds[&node.id()].height
}
#[test]
fn normal_uses_backend_metrics_but_authored_line_height_remains_authoritative() {
    assert_eq!(height(""), 30.0);
    assert_eq!(height("line-height:1.2"), 24.0);
    assert_eq!(height("line-height:40px"), 40.0);
    assert_eq!(height("line-height:0"), 0.0);
    assert_eq!(height("zoom:.5"), 15.0);
}

#[test]
fn inherited_normal_is_resolved_again_for_the_child_font() {
    let page = Page::parse(
        "<style>body{margin:0}div{font:20px Arial}span{font-size:30px}</style><div><span>Agjp</span></div>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("div").next().unwrap();
    let layout = layout_page(&page, 800.0, 600.0, &mut FontMetricsMeasurer);
    assert_eq!(layout.node_bounds[&node.id()].height, 45.0);
}

#[test]
fn script_geometry_uses_the_same_normal_line_height_as_rendering() {
    let page = Page::parse(
        "<style>body{margin:0}div{font:20px Arial}</style><div>Agjp</div>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("div").next().unwrap();
    let geometry =
        layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FontMetricsMeasurer);
    assert_eq!(geometry.node_bounds[&node.id()].height, 30.0);
}
