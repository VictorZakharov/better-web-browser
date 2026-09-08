use super::test_support::FixedMeasurer;
use super::*;

#[test]
fn out_of_flow_decorations_do_not_inflate_a_flex_items_intrinsic_width() {
    for position in ["absolute", "fixed"] {
        let page = Page::parse(
            &format!(
                r#"<style>
            body {{ margin: 0 }} main {{ display:flex; width:1000px }}
            #avatar {{ position:relative; flex:none; margin-right:16px }}
            button {{ width:36px; height:36px; padding:0; border:0 }}
            #decoration {{ position:{position}; width:100%; height:100% }}
            #decoration span {{ display:inline-block; width:50%; border-left:1px solid }}
            #text {{ flex:1; min-width:0 }}
        </style><main><div id=avatar><button></button><div id=decoration><span></span></div></div>
        <div id=text>Comment</div></main>"#
            ),
            "https://example.com/",
        );
        let output = layout_page(&page, 1000.0, 600.0, &mut FixedMeasurer);
        let rect = |id| {
            let node = page
                .dom
                .elements_named("div")
                .find(|node| node.attr("id").as_deref() == Some(id))
                .unwrap();
            output.node_bounds[&node.id()]
        };
        assert_eq!(rect("avatar").width, 36.0, "{position}");
        assert_eq!(rect("text").x, 52.0, "{position}");
        assert!(
            rect("decoration").width > 0.0,
            "out-of-flow decoration still gets its own box"
        );
    }
}

#[test]
fn text_paint_centers_font_metrics_in_extra_line_leading() {
    let page = Page::parse(
        "<div style='font-size:20px;line-height:40px'>Label</div>",
        "https://example.com/",
    );
    let element = page.dom.elements_named("div").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let line = output.node_bounds[&element.id()];
    let text = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text { rect, text, .. } if text == "Label" => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(line.height, 40.0);
    assert_eq!(text.y - line.y, 10.0);
    assert_eq!(text.height, 20.0);
}

#[test]
fn nested_block_intrinsic_width_does_not_use_descendant_flex_basis() {
    let page = Page::parse(
        "<div style='display:flex'><div><div><button style='display:flex;flex:1 1 0%;padding:0;border:0'><span>Subscribe</span></button></div></div></div>",
        "https://example.com/",
    );
    let button = page.dom.elements_named("button").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert!(output.node_bounds[&button.id()].width > 50.0);
}

#[test]
fn cyclic_percentage_maximum_does_not_erase_intrinsic_button_width() {
    for maximum in ["100%", "calc(100% - 2px)"] {
        let page = Page::parse(
            &format!(
                "<div style='display:flex'><div><div style='display:flex'><span style='max-width:{maximum}'><div><div style='display:flex;max-width:{maximum}'><button style='display:flex;flex:1 1 0%;padding:0;border:0'><span>Subscribe</span></button></div></div></span></div></div></div>"
            ),
            "https://example.com/",
        );
        let button = page.dom.elements_named("button").next().unwrap();
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert!(output.node_bounds[&button.id()].width > 50.0, "{maximum}");
    }
}

#[test]
fn authored_flex_button_preserves_child_layout_and_form_metadata() {
    let page = Page::parse(
        "<form><button name='action' value='save' style='display:flex;width:140px;height:40px;gap:8px'><span style='width:20px;height:20px;background:red'></span><span>Save</span></button></form>",
        "https://example.com/",
    );
    let button = page.dom.elements_named("button").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert!(
        output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "Save"))
    );
    for child in Node::composed_children(&button) {
        assert!(output.node_bounds.contains_key(&child.id()));
    }
    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) if spec.node_id == button.id() => Some(spec),
            _ => None,
        })
        .unwrap();
    assert!(control.authored_content);
    assert_eq!(control.value, "save");
    assert!(control.form_id.is_some());
}

#[test]
fn percentage_image_uses_definite_inline_block_content_box() {
    let page = Page::parse(
        "<span style='display:inline-block;width:44px;height:44px;padding:2px;box-sizing:border-box'><img src='avatar.png' style='width:100%;height:100%'></span>",
        "https://example.com/",
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let rect = output.node_bounds[&image.id()];
    assert_eq!(rect.width, 40.0);
    assert_eq!(rect.height, 40.0);
}

#[test]
fn replaced_image_maximum_width_shrinks_intrinsic_ratio_inside_inline_block() {
    let mut page = Page::parse(
        "<span style='display:inline-block;width:40px;height:40px'><img src='avatar.png' style='max-width:100%;height:auto'></span>",
        "https://example.com/",
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let key = page.image_url(&image).unwrap();
    page.images.insert(
        key,
        crate::engine::DecodedImage {
            width: 88,
            height: 88,
            bgra: vec![255; 88 * 88 * 4].into(),
        },
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(output.node_bounds[&image.id()].width, 40.0);
    assert_eq!(output.node_bounds[&image.id()].height, 40.0);
}

#[test]
fn text_input_auto_height_does_not_duplicate_css_padding() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            input { display: block; font-size: 16px; line-height: 20px;
                    padding: 1px 0; border-width: 1px; }
        </style><input id="search" placeholder="Search">"#,
        "https://example.com/",
    );
    let input = page.dom.elements_named("input").next().unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) if spec.node_id == input.id() => Some(spec),
            _ => None,
        })
        .unwrap();
    assert_eq!(control.rect.height, 24.0);
    assert_eq!(output.node_bounds[&input.id()].height, 24.0);
}
