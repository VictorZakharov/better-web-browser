use super::test_support::FixedMeasurer;
use super::*;

mod flex_anonymous;

#[test]
fn centers_a_fit_content_grid_item_across_its_spanned_area() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .grid { display: grid; grid-template-columns: repeat(3, 100px); width: 300px }
            .search { grid-column: 1 / 4; justify-self: center; height: 20px; background: red }
            .search-core { width: 120px; height: 20px }
           </style><div class="grid"><div id="search" class="search">
             <div class="search-core"></div>
           </div></div>"#,
        "https://example.com/",
    );
    let search = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("search"))
        .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);

    assert_eq!(
        output.node_bounds.get(&search.id()),
        Some(&RectF {
            x: 90.0,
            y: 0.0,
            width: 120.0,
            height: 20.0,
        })
    );
}

#[test]
fn paints_css_masks_with_the_elements_background_color() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .icon { display: block; width: 20px; height: 20px;
                    background-color: #36c; mask-image: url('/menu.svg') }
        </style><span class="icon"></span>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);

    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Image { url, tint: Some(color), .. }
            if url == "https://example.com/menu.svg" && *color == Color::rgb(51, 102, 204))
    }));
    assert!(!output.items.iter().any(|item| {
        matches!(item, DisplayItem::SolidRect { color, .. }
            if *color == Color::rgb(51, 102, 204))
    }));
}

#[test]
fn resolves_percentage_radius_against_the_finished_box() {
    let page = Page::parse(
        r#"<style>body{margin:0}.pill{width:100px;height:40px;background:red;border-radius:50%}</style>
           <div class="pill"></div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let radius = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { radius, .. } => Some(*radius),
            _ => None,
        })
        .unwrap();
    assert_eq!(radius, 20.0);
}

#[test]
fn centers_flex_items_with_automatic_inline_margins() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .row { display: flex; width: 300px }
            .item { width: 100px; height: 20px; margin: 0 auto; background: red }
           </style><div class="row"><div class="item"></div></div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let item = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(item.x, 100.0);
}

#[test]
fn lays_out_inline_flex_children_on_the_inline_axis() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .row { display: flex }
            .nav { display: inline-flex; margin: 0; padding: 0 }
            .nav > li { display: block; width: 50px; height: 20px }
           </style>
           <div class="row"><ul class="nav"><li id="first">One</li><li id="second">Two</li></ul></div>"#,
        "https://example.com/",
    );
    let first = page
        .dom
        .elements_named("li")
        .find(|node| node.attr("id").as_deref() == Some("first"))
        .unwrap();
    let second = page
        .dom
        .elements_named("li")
        .find(|node| node.attr("id").as_deref() == Some("second"))
        .unwrap();
    let styles = page.style_for_viewport(300.0, 200.0);
    let nav = page.dom.elements_named("ul").next().unwrap();
    assert_eq!(styles.get(&nav).display, Display::InlineFlex);

    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let first = output.node_bounds[&first.id()];
    let second = output.node_bounds[&second.id()];
    assert_eq!(first.y, second.y);
    assert_eq!(second.x, first.right());
}

#[test]
fn treats_indefinite_percentage_heights_as_auto_and_hides_zero_max_height_overflow() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .column { display: flex; flex-direction: column; width: 200px }
            .indefinite { height: 100%; background: red }
            .collapsed { max-height: 0; overflow: hidden }
            .after { height: 20px; background: blue }
           </style><div class="column">
             <div class="indefinite"></div>
             <div class="collapsed">must not paint</div>
             <div class="after"></div>
           </div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let after = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(0, 0, 255) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(after.y, 0.0);
    assert!(!output.items.iter().any(
        |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("must not paint"))
    ));
}

#[test]
fn preserves_textarea_semantics_for_native_controls() {
    let page = Page::parse(
        r#"<form action="/search"><textarea name="q" rows="1">hello</textarea></form>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) => Some(control),
            _ => None,
        })
        .unwrap();
    assert_eq!(control.kind, ControlKind::TextArea);
    assert_eq!(control.name, "q");
    assert_eq!(control.value, "hello");
}

#[test]
fn preserves_block_level_replaced_form_controls() {
    let page = Page::parse(
        r#"<style>body{margin:0}input{display:block;width:100%;height:44px;border:0}</style>
           <form action="/search"><input name="q" value="test"></form>"#,
        "https://example.com/",
    );
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
    assert_eq!(control.kind, ControlKind::Text);
    assert_eq!(control.name, "q");
    assert_eq!(control.value, "test");
    assert_eq!(control.rect.width, 300.0);
    assert_eq!(control.rect.height, 44.0);
}

#[test]
fn represents_select_as_one_native_control_instead_of_all_option_text() {
    let page = Page::parse(
        r#"<style>body{margin:0}</style><form action="/search">
           <select name="region"><option value="all">All Regions</option>
           <option value="ca" selected>Canada</option></select></form>"#,
        "https://example.com/",
    );
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
    assert_eq!(control.kind, ControlKind::Select);
    assert_eq!(control.name, "region");
    assert_eq!(control.value, "ca");
    assert_eq!(control.label, "Canada");
    assert_eq!(control.options.len(), 2);
    assert!(!output.items.iter().any(
        |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("All RegionsCanada"))
    ));
}

#[test]
fn keeps_transparent_borders_in_layout_without_painting_them() {
    let page = Page::parse(
        r#"<style>body{margin:0}.result{height:20px;border:1px solid rgba(0,0,0,0)}</style>
           <div class="result"></div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    assert!(
        !output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::BorderRect { .. }))
    );
}

#[test]
fn renders_noscript_fallback_when_script_execution_is_unavailable() {
    let page = Page::parse(
        r#"
            <script>script-only text</script>
            <noscript>
                <style>div { display:none }</style>
                <div style="display:block">Script-free fallback</div>
            </noscript>
        "#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Script-free fallback");
}

#[test]
fn honors_hidden_list_markers_and_clips_accessibility_text() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .toc li { list-style-type: none }
            .visually-hidden { display: block; position: absolute; width: 1px; height: 1px;
                               overflow: hidden }
           </style>
           <ul class="toc"><li>Contents item</li></ul>
           <label><span>Visible label</span><span class="visually-hidden">Accessibility label</span></label>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(text, "Contents itemVisible label");
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
    assert!(control.label.is_empty());
    assert!(
        control
            .icon_url
            .as_deref()
            .is_some_and(|url| url.starts_with("data:"))
    );
}

#[test]
fn paints_block_level_replaced_images_at_their_specified_size() {
    let mut page = Page::parse(
        r#"<style>img { display: block; width: 40px; height: 20px }</style>
           <img src="logo.png" alt="Logo">"#,
        "https://example.com/",
    );
    page.add_image("https://example.com/logo.png".into(), &{
        let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    })
    .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 300.0, 200.0, &mut measurer);
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Image { rect, url, .. }
            if url.ends_with("logo.png") && rect.width == 40.0 && rect.height == 20.0)
    }));
}

#[test]
fn renders_shadow_content_and_assigned_nodes_once_in_composed_order() {
    let mut page = Page::parse(
        r#"<x-card><strong slot="title">Light title</strong><span>Light body</span></x-card>
            <p>After</p>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-card").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        r#"<style>.frame { display: block }</style><div class="frame">
            <slot name="title">Fallback title</slot><slot>Fallback body</slot>
        </div>"#,
        true,
    );
    page.refresh_resources(800.0);
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let text = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");

    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(normalized.contains("Light title"), "{text}");
    assert!(normalized.contains("Light body"), "{text}");
    assert!(text.contains("After"), "{text}");
    assert!(!text.contains("Fallback"), "{text}");
    assert_eq!(normalized.matches("Light title").count(), 1, "{text}");
}

#[test]
fn shadow_slots_flatten_assigned_block_and_flex_boxes() {
    let mut page = Page::parse(
        r#"<x-provider style="display:block">
              <section id="assigned" style="display:flex;width:300px">
                <div style="width:100px;height:20px;background:#f00"></div>
                <div style="width:200px;height:20px;background:#00f"></div>
              </section>
            </x-provider>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-provider").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(&root, "<slot></slot>", true);
    page.refresh_resources(800.0);

    let assigned = page
        .dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("assigned"))
        .unwrap();
    let slot = Node::descendants(&root)
        .find(|node| node.tag_name() == Some("slot"))
        .unwrap();
    let styles = page.style_for_viewport(800.0, 600.0);
    assert_eq!(styles.get(&slot).display, Display::Contents);

    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert_eq!(
        output.node_bounds.get(&assigned.id()),
        Some(&RectF {
            x: 8.0,
            y: 8.0,
            width: 300.0,
            height: 20.0,
        })
    );
}
