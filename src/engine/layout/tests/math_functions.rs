use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn measured(source: &str, width: f32, height: f32) -> HashMap<String, RectF> {
    let page = Page::parse(source, "https://example.com/");
    let output = layout_page(&page, width, height, &mut FixedMeasurer);
    page.dom
        .elements_named("div")
        .filter_map(|node| Some((node.attr("id")?, *output.node_bounds.get(&node.id())?)))
        .collect()
}

#[test]
fn quantized_widths_respond_to_containing_width_changes() {
    let source = r#"<style>body{margin:0}#parent{width:80%}
        #round{width:round(50%,30px);height:10px}
        #mod{width:mod(50%,70px);height:10px}
        #hypot{width:hypot(30px,10%);height:10px}
        </style><div id=parent><div id=round></div><div id=mod></div><div id=hypot></div></div>"#;
    for viewport in [200.0f32, 400.0, 800.0] {
        let output = measured(source, viewport, 600.0);
        let half = viewport * 0.4;
        assert!((output["round"].width - (half / 30.0).round() * 30.0).abs() < 0.02);
        assert!((output["mod"].width - half % 70.0).abs() < 0.02);
        assert!((output["hypot"].width - 30.0f32.hypot(viewport * 0.08)).abs() < 0.02);
    }
}

#[test]
fn trigonometric_translation_uses_the_actual_percentage_basis() {
    let source = r#"<style>body{margin:0}#parent{position:relative;width:200px;height:100px}
        #child{position:absolute;left:calc(sin(atan2(30%,40%)) * 100px);top:round(up,25%,10px);width:10px;height:10px}
        </style><div id=parent><div id=child></div></div>"#;
    let output = measured(source, 800.0, 600.0);
    assert!((output["child"].x - 60.0).abs() < 0.02);
    assert!((output["child"].y - 30.0).abs() < 0.02);
}

#[test]
fn nan_width_is_zero_not_auto_or_an_earlier_declaration() {
    let source = r#"<style>body{margin:0}#box{width:100px;width:calc(sqrt(-1) * 10px);height:10px}
        #other{width:100px;width:mod(20px,0px);height:10px}</style><div id=box></div><div id=other></div>"#;
    let output = measured(source, 800.0, 600.0);
    assert_eq!(output["box"].width, 0.0);
    assert_eq!(output["other"].width, 0.0);
}
