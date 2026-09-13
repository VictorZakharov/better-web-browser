use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn page(display: &str, position: &str) -> Page {
    Page::parse(
        &format!(
            r#"<style>
          body {{ margin:0 }} form {{ position:relative; width:228px }}
          button {{ display:{display}; position:{position}; padding:0; border:0;
            min-width:22px; min-height:22px; box-sizing:border-box; overflow:hidden }}
          .icon {{ display:inline-block; width:12px; height:12px; background:red }}
          .label {{ position:absolute; width:1px; height:1px; margin:-1px;
            overflow:hidden }}
        </style><form><button name=action value=expand><span class=icon></span><span
          class=label>Expand section</span></button></form>"#
        ),
        "https://example.test/",
    )
}

#[test]
fn block_buttons_preserve_descendant_clipping_and_form_metadata() {
    for display in ["block", "flow-root", "flex", "grid"] {
        let page = page(display, "absolute");
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let button = page.dom.elements_named("button").next().unwrap();
        let control = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) if control.node_id == button.id() => Some(control),
                _ => None,
            })
            .unwrap();
        assert!(
            control.authored_content,
            "{display}: native text must not repaint hidden labels"
        );
        assert_eq!(control.value, "expand");
        assert!(control.form_id.is_some());
        assert!(
            output.items.iter().any(|item| matches!(item,
                DisplayItem::BeginClip { bounds } if bounds.width == 1.0 && bounds.height == 1.0
            )),
            "{display}: hidden label retains its own CSS clip"
        );
    }
}

#[test]
fn automatic_block_button_inline_size_is_fit_content() {
    for position in ["static", "absolute"] {
        let page = page("block", position);
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let button = page.dom.elements_named("button").next().unwrap();
        let bounds = output.node_bounds[&button.id()];
        assert_eq!(
            bounds.width, 22.0,
            "{position}: positioned label is not intrinsic content"
        );
        assert!(
            bounds.height <= 23.0,
            "{position}: no native-control height: {bounds:?}"
        );
        let icon = page.dom.elements_named("span").next().unwrap();
        assert_eq!(output.node_bounds[&icon.id()].width, 12.0);
    }
}

#[test]
fn default_button_box_sizing_includes_padding_in_minimum_size() {
    let page = Page::parse(
        r#"<style>button{display:block;padding:4px;border:1px solid;min-width:22px;
          min-height:22px} span{display:block;width:12px;height:12px}</style>
          <button><span></span></button>"#,
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let button = page.dom.elements_named("button").next().unwrap();
    let bounds = output.node_bounds[&button.id()];
    assert_eq!((bounds.width, bounds.height), (22.0, 22.0));
}
