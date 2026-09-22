use super::*;
use crate::engine::layout::test_support::FixedMeasurer;
mod alignment;
mod form_fidelity;
mod select;

#[test]
fn transparent_edit_preserves_a_rounded_ancestor_background() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .search { display: flex; width: 200px; height: 40px;
                      background: #333; border-radius: 24px }
            input { width: 100%; height: 100%; border: 0; padding: 0;
                    background: transparent; color: white }
           </style><div class="search"><input aria-label="Search"></div>"#,
        "https://example.test/",
    );
    let output = layout_page(&page, 300.0, 200.0, &mut FixedMeasurer);
    let input = page.dom.elements_named("input").next().unwrap();
    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.node_id == input.id() => Some(control),
            _ => None,
        })
        .unwrap();
    assert_eq!(control.background_color.alpha, 0);
    assert!(output.items.iter().any(|item| matches!(item,
        DisplayItem::SolidRect { radius, .. } if *radius == 20.0
    )));
}

#[test]
fn icon_only_buttons_use_mask_descendants_without_accessibility_text() {
    let mut page = Page::parse(
        r#"<style>
            .icon { width: 20px; height: 20px; background-color: black;
                    mask-image: url('data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%2220%22 height=%2220%22%3E%3Cpath d=%22M0 0h10v20H0z%22/%3E%3C/svg%3E') }
            .label { display: block; position: absolute; width: 1px; height: 1px;
                     overflow: hidden }
           </style><button><span class="icon"></span><span class="label">Toggle menu</span></button>"#,
        "https://example.com/",
    );
    page.refresh_resources(300.0);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) => Some(control),
            _ => None,
        })
        .unwrap();
    assert!(
        control.authored_content,
        "native label must not be repainted"
    );
    assert!(
        output.items.iter().any(|item| matches!(item,
            DisplayItem::Image { url, tint: Some(_), .. } if url.starts_with("data:")
        )),
        "the authored mask descendant must paint"
    );
    assert!(
        output.items.iter().any(|item| matches!(item,
            DisplayItem::BeginClip { bounds } if bounds.width == 1.0 && bounds.height == 1.0
        )),
        "the accessibility label retains its CSS clip"
    );
}
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
fn inline_buttons_use_measured_children_without_a_native_minimum_width() {
    let page = Page::parse(
        "<style>body{margin:0}button{font:12px/14px sans-serif;padding:2px 4px;border:0}span{display:inline-block;width:10px;height:10px;background:red}</style><form><button name=action value=hide>hide</button><button><span></span></button></form>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let buttons: Vec<_> = page.dom.elements_named("button").collect();
    let first = output.node_bounds[&buttons[0].id()];
    let second = output.node_bounds[&buttons[1].id()];
    assert!(first.width < 45.0 && first.width > 20.0, "{first:?}");
    assert_eq!(second.width, 18.0);
    for button in buttons {
        assert!(output.items.iter().any(|item| matches!(item,DisplayItem::Control(spec) if spec.node_id==button.id() && spec.authored_content && spec.form_id.is_some())));
    }
    assert!(
        output
            .items
            .iter()
            .any(|item| matches!(item,DisplayItem::Text{text,..} if text=="hide"))
    );
}

#[test]
fn inline_buttons_preserve_wrapping_positioned_labels_and_overflow() {
    let page = page("inline-block", "relative");
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let button = page.dom.elements_named("button").next().unwrap();
    assert_eq!(output.node_bounds[&button.id()].width, 22.0);
    assert!(output.items.iter().any(|item|matches!(item,DisplayItem::BeginClip{bounds} if bounds.width==1.0 && bounds.height==1.0)));
    let page = Page::parse(
        "<style>button{width:100px;font:16px/20px sans-serif;padding:0;border:0}</style><button>one two three four five six seven eight nine ten</button>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let button = page.dom.elements_named("button").next().unwrap();
    assert_eq!(output.node_bounds[&button.id()].width, 100.0);
    assert!(output.node_bounds[&button.id()].height > 40.0);
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
