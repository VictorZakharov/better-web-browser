use super::*;

#[test]
fn flex_cross_alignment_uses_constrained_height_not_natural_height() {
    for (height, alignment, expected) in [
        ("min-height:60px", "center", 20.0),
        ("min-height:60px", "flex-end", 40.0),
        ("height:60px", "center", 20.0),
        ("min-height:60px;max-height:10px", "center", 20.0),
        ("max-height:10px", "center", -5.0),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{display:flex;align-items:{alignment};{height}}}\
             span{{display:block;width:20px;height:20px}}</style><main><span></span></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let span = page.dom.elements_named("span").next().unwrap();
        assert_eq!(
            output.node_bounds[&span.id()].y,
            expected,
            "{height} {alignment}"
        );
    }
}

#[test]
fn placeholder_cascade_preserves_entered_text_color_and_never_generates_content() {
    let page = Page::parse(
        "<style>input{color:black;--hint:#72777d}input::placeholder{color:var(--hint);content:'wrong'}\
         textarea::placeholder{color:blue;opacity:.5}</style>\
         <input placeholder=Search><input value=Entered placeholder=Search><textarea placeholder=Notes></textarea>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let controls = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Control(control) => Some(control),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(controls.len(), 3);
    for control in &controls[..2] {
        assert_eq!(control.text_color, Color::BLACK);
        assert_eq!(control.placeholder_color, Color::rgb(114, 119, 125));
    }
    assert_eq!(controls[1].value, "Entered");
    assert_eq!(controls[2].placeholder_color, Color::rgb(127, 127, 255));
    assert!(
        !output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "wrong"))
    );
}

#[test]
fn checked_sibling_cascade_tracks_live_state_without_attribute_changes() {
    let page = Page::parse(
        "<style>span{display:inline-block;width:20px;height:20px;background:red}\
         input:checked+span{background:blue}</style>\
         <input type=radio name=size checked><span></span><input type=radio name=size><span></span>",
        "https://example.test/",
    );
    let inputs = page.dom.elements_named("input").collect::<Vec<_>>();
    let spans = page.dom.elements_named("span").collect::<Vec<_>>();
    let styles = page.style(800.0);
    assert_eq!(
        styles.get(&spans[0]).background_color,
        Color::rgb(0, 0, 255)
    );
    inputs[1].set_checked(true, true);
    let styles = page.style(800.0);
    assert_eq!(
        styles.get(&spans[0]).background_color,
        Color::rgb(255, 0, 0)
    );
    assert_eq!(
        styles.get(&spans[1]).background_color,
        Color::rgb(0, 0, 255)
    );
    assert!(inputs[0].attr("checked").is_some());
    assert!(inputs[1].attr("checked").is_none());
}

#[test]
fn placeholder_current_color_inherits_from_the_input_not_the_ua_hint() {
    for declaration in [
        "color:currentColor",
        "color:inherit",
        "color:red;color:currentColor !important",
    ] {
        let page = Page::parse(
            &format!(
                "<style>input{{color:blue}}input::placeholder{{{declaration}}}</style><input placeholder=Hint>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let hint = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) => Some(control.placeholder_color),
                _ => None,
            })
            .unwrap();
        assert_eq!(hint, Color::rgb(0, 0, 255), "{declaration}");
    }
}
