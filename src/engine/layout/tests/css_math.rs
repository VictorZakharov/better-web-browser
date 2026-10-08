//! Used-value comparisons across ordinary layout contexts, not a game override.
use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn boxes(source: &str, width: f32, height: f32) -> HashMap<String, RectF> {
    let page = Page::parse(source, "https://example.com/");
    let output = layout_page(&page, width, height, &mut FixedMeasurer);
    page.dom
        .elements_named("div")
        .filter_map(|node| Some((node.attr("id")?, *output.node_bounds.get(&node.id())?)))
        .collect()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.02,
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn responsive_aspect_ratio_container_does_not_fill_the_viewport() {
    let source = r#"<style>body{margin:0}
        #logo{position:relative;width:min(620px,88vw);aspect-ratio:700 / 330}
        svg{position:absolute;inset:0;width:100%;height:100%}
        #following{height:20px}
        </style><div id=logo><svg viewBox="0 0 700 330"><rect width=700 height=330/></svg></div><div id=following></div>"#;
    for (viewport, expected) in [(1262.0, 620.0), (500.0, 440.0), (300.0, 264.0)] {
        let page = Page::parse(source, "https://example.com/");
        let output = layout_page(&page, viewport, 539.0, &mut FixedMeasurer);
        let logo = page.dom.elements_named("div").next().unwrap();
        let svg = page.dom.elements_named("svg").next().unwrap();
        let logo = output.node_bounds[&logo.id()];
        let svg = output.node_bounds[&svg.id()];
        close(logo.width, expected);
        close(logo.height, expected * 330.0 / 700.0);
        close(svg.width, logo.width);
        close(svg.height, logo.height);
        let following = page.dom.elements_named("div").last().unwrap();
        close(output.node_bounds[&following.id()].y, logo.bottom());
    }
}

#[test]
fn containing_width_and_viewport_width_are_distinct_math_inputs() {
    let source = r#"<style>body{margin:0}#parent{width:400px}
        #percent{width:min(50%,300px);height:10px}
        #viewport{width:min(50vw,300px);height:10px}
        </style><div id=parent><div id=percent></div><div id=viewport></div></div>"#;
    for (viewport, expected) in [(1000.0, 300.0), (500.0, 250.0), (300.0, 150.0)] {
        let output = boxes(source, viewport, 600.0);
        close(output["percent"].width, 200.0);
        close(output["viewport"].width, expected);
    }
}

#[test]
fn comparison_height_uses_the_definite_containing_height() {
    let output = boxes(
        r#"<style>body{margin:0}#parent{width:300px;height:200px}
        #child{width:40px;height:clamp(20px,50%,180px)}
        </style><div id=parent><div id=child></div></div>"#,
        800.0,
        600.0,
    );
    close(output["child"].height, 100.0);
}

#[test]
fn short_viewport_comparison_arithmetic_constrains_both_axes() {
    let source = r#"<style>body{margin:0}
        #logo{width:min(620px,88vw,(100vh - 190px)*2.12);aspect-ratio:700 / 330}
        </style><div id=logo></div>"#;
    for (height, expected) in [(600.0, 620.0), (400.0, 445.2), (300.0, 233.2)] {
        let output = boxes(source, 1262.0, height);
        close(output["logo"].width, expected);
        close(output["logo"].height, expected * 330.0 / 700.0);
    }
}

#[test]
fn min_max_constraints_are_applied_after_the_comparison() {
    let output = boxes(
        r#"<style>body{margin:0}
        #small{width:min(20px,30px);min-width:100px;height:10px}
        #large{width:max(200px,300px);max-width:100px;height:10px}
        #reversed{width:clamp(200px,10px,100px);height:10px}
        </style><div id=small></div><div id=large></div><div id=reversed></div>"#,
        800.0,
        600.0,
    );
    close(output["small"].width, 100.0);
    close(output["large"].width, 100.0);
    close(output["reversed"].width, 200.0);
}

#[test]
fn margin_and_padding_shorthands_keep_function_arguments_together() {
    let output = boxes(
        r#"<style>body{margin:0}
        #box{width:100px;height:40px;margin:min(10px,20px) max(20px,30px);
          padding:clamp(2px,5px,10px) min(8px,9px)}
        </style><div id=box></div>"#,
        800.0,
        600.0,
    );
    close(output["box"].x, 30.0);
    close(output["box"].y, 10.0);
    close(output["box"].width, 116.0);
    close(output["box"].height, 50.0);
}

#[test]
fn malformed_length_shorthands_leave_the_previous_declaration_intact() {
    let output = boxes(
        r#"<style>body{margin:0}
        #box{width:100px;height:40px;margin:10px 20px;padding:5px;
          margin:1px bogus;padding:2px 3px 4px 5px 6px;width:30px junk}
        </style><div id=box></div>"#,
        800.0,
        600.0,
    );
    close(output["box"].x, 20.0);
    close(output["box"].y, 10.0);
    close(output["box"].width, 110.0);
    close(output["box"].height, 50.0);
}
