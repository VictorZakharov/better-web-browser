use super::test_support::FixedMeasurer;
use super::*;

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
