use super::*;

#[test]
fn block_content_alignment_moves_content_not_the_containing_box() {
    for (value, height, expected) in [
        ("center", 100, 40.0),
        ("end", 100, 80.0),
        ("start", 100, 0.0),
        ("normal", 100, 0.0),
        ("space-around", 100, 40.0),
        ("space-between", 100, 0.0),
        ("safe center", 10, 0.0),
        ("unsafe center", 10, -5.0),
        ("center", 10, 0.0),
    ] {
        let page = Page::parse(
            &format!(
                r#"<style>body{{margin:0}}
            main{{position:relative;height:{height}px;width:100px;align-content:{value}}}
            #flow{{height:20px}} #positioned{{position:absolute;top:0;height:5px;width:5px}}
            </style><main><div id=flow></div><div id=positioned></div></main>"#
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let bounds = |id| {
            output.node_bounds[&Node::descendants(&page.dom.document)
                .find(|node| node.attr("id").as_deref() == Some(id))
                .unwrap()
                .id()]
        };
        assert_eq!(bounds("flow").y, expected, "{value}/{height}");
        assert_eq!(bounds("positioned").y, 0.0, "positioned CB must not move");
        let main = page.dom.elements_named("main").next().unwrap();
        assert_eq!(output.node_bounds[&main.id()].height, height as f32);
    }
}

#[test]
fn alignment_creates_a_formatting_context_and_contains_child_margins() {
    let page = Page::parse(
        r#"<style>body{margin:0}
        main{height:100px;align-content:center} div{height:20px;margin:10px 0}
        </style><main><div></div></main>"#,
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let main = page.dom.elements_named("main").next().unwrap();
    let child = page.dom.elements_named("div").next().unwrap();
    assert_eq!(output.node_bounds[&main.id()].y, 0.0);
    assert_eq!(output.node_bounds[&child.id()].y, 40.0);
}

#[test]
fn button_alignment_defaults_allow_author_override_and_css_wide_values() {
    for (declaration, expected) in [
        ("", 40.0),
        ("align-content:start", 0.0),
        ("align-content:initial", 0.0),
        ("align-content:unset", 0.0),
        ("align-content:start;align-content:revert", 40.0),
        ("align-content:inherit", 80.0),
    ] {
        let page = Page::parse(
            &format!(
                r#"<style>body{{margin:0;align-content:end}}
            button{{height:100px;width:100px;padding:0;border:0;{declaration}}}
            span{{display:block;height:20px}}
            </style><button><span></span></button>"#
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let button = page.dom.elements_named("button").next().unwrap();
        let child = page.dom.elements_named("span").next().unwrap();
        assert_eq!(
            output.node_bounds[&child.id()].y - output.node_bounds[&button.id()].y,
            expected,
            "{declaration}"
        );
    }
}
