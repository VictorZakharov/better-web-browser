use super::*;

#[test]
fn block_svg_is_replaced_content_with_percentage_dimensions() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .frame { width: 200px; height: 100px }
            svg { display: block; width: 50%; height: 100% }
        </style>
        <div class="frame"><svg id="icon" viewBox="0 0 20 10">
            <rect width="20" height="10" />
        </svg></div>"#,
        "https://example.com/",
    );
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(
        output.node_bounds.get(&node_id(&svg)).copied(),
        Some(RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        })
    );
    assert!(output.items.iter().any(|item| matches!(
        item,
        DisplayItem::Image { rect, url, .. }
            if url == &key && rect.width == 100.0 && rect.height == 100.0
    )));
}

#[test]
fn inline_svg_percentages_use_the_definite_containing_block() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .frame { width: 240px; height: 80px }
            svg { width: 25%; height: 50% }
        </style>
        <div class="frame"><svg id="icon" viewBox="0 0 20 10">
            <rect width="20" height="10" />
        </svg></div>"#,
        "https://example.com/",
    );
    let svg = page.dom.elements_named("svg").next().unwrap();
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let bounds = output.node_bounds.get(&node_id(&svg)).copied().unwrap();

    assert_eq!(bounds.width, 60.0);
    assert_eq!(bounds.height, 40.0);
}

#[test]
fn svg_presentation_dimensions_preserve_the_viewbox_ratio() {
    let page = Page::parse(
        r#"<style>body { margin: 0 }</style>
        <svg width="120" viewBox="0 0 30 10"><rect width="30" height="10" /></svg>"#,
        "https://example.com/",
    );
    let svg = page.dom.elements_named("svg").next().unwrap();
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let bounds = output.node_bounds.get(&node_id(&svg)).copied().unwrap();

    assert_eq!(bounds.width, 120.0);
    assert_eq!(bounds.height, 40.0);
}

#[test]
fn percentage_sized_form_controls_use_their_definite_containing_block() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .host { width: 200px; height: 100px }
            input, textarea, select, button {
                width: 50%; height: 50%; box-sizing: border-box;
                margin: 0; padding: 0; border-width: 0;
            }
        </style>
        <div class="host">
            <input id="input"><br><textarea id="textarea"></textarea><br>
            <select id="select"><option>One</option></select><br><button id="button">Go</button>
        </div>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    let controls = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Control(spec) => Some(spec),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(controls.len(), 4);
    assert!(controls.iter().all(|spec| spec.rect.width == 100.0));
    assert!(controls.iter().all(|spec| spec.rect.height == 50.0));
    for tag in ["input", "textarea", "select", "button"] {
        let node = page.dom.elements_named(tag).next().unwrap();
        assert_eq!(
            output.node_bounds[&node_id(&node)],
            controls
                .iter()
                .find(|spec| spec.node_id == node.id())
                .unwrap()
                .rect
        );
    }
}

#[test]
fn icon_button_paints_a_shadow_hydrated_svg_inside_its_control_box() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .host { width: 40px; height: 40px }
            button { width: 100%; height: 100%; box-sizing: border-box;
                margin: 0; padding: 0; border-width: 0; }
            svg { width: 24px; height: 24px }
        </style>
        <div class="host"><button aria-label="Menu"><svg viewBox="0 0 24 24">
            <path d="M3 6h18v2H3zm0 5h18v2H3zm0 5h18v2H3z" />
        </svg></button></div>"#,
        "https://example.com/",
    );
    let button = page.dom.elements_named("button").next().unwrap();
    let svg = page.dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    let mut measurer = FixedMeasurer;

    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) if spec.node_id == button.id() => Some(spec),
            _ => None,
        })
        .unwrap();

    assert_eq!(control.rect.width, 40.0);
    assert_eq!(control.rect.height, 40.0);
    assert_eq!(control.icon_url.as_deref(), Some(key.as_str()));
    assert_eq!(control.icon_width, 24.0);
    assert_eq!(control.icon_height, 24.0);
    assert_eq!(page.image_url(&svg).as_deref(), Some(key.as_str()));
}
