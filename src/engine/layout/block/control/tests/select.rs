use super::*;

#[test]
fn empty_input_button_values_do_not_paint_accessible_names_as_labels() {
    let page = Page::parse(
        r#"<input type=submit value="" title=Search aria-label=Search>
        <input type=button value="" title=More><input type=submit><input type=reset>"#,
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let labels: Vec<_> = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Control(spec) => Some(spec.label.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["", "", "Submit", "Reset"]);
}

#[test]
fn select_options_remain_native_content_after_blockification() {
    for parent in ["block", "flex", "grid"] {
        for display in ["block", "inline-block", "flex", "grid"] {
            let page = Page::parse(
                &format!(
                    r#"<style>
                body{{margin:0}}form{{display:{parent};width:400px}}
                select{{display:{display};width:140%;font:16px/20px sans-serif;padding:0;border:0}}
                option{{display:block;height:100px;position:relative}}
                </style><form><select name=region><option value=a>First</option>
                <optgroup label=Group><option value=b selected label=Chosen>Second</option></optgroup>
                <option value=c>Third</option></select></form>"#
                ),
                "https://example.test/",
            );
            let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
            let controls: Vec<_> = output
                .items
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Control(spec) => Some(spec),
                    _ => None,
                })
                .collect();
            assert_eq!(controls.len(), 1, "{parent} {display}");
            let control = controls[0];
            assert_eq!(control.kind, ControlKind::Select);
            assert_eq!(control.value, "b");
            assert_eq!(control.label, "Chosen");
            assert_eq!(control.selected_index, 1);
            assert_eq!(control.options.len(), 3);
            assert!(control.form_id.is_some());
            assert!(
                control.rect.height <= 32.0,
                "{parent} {display}: {:?}",
                control.rect
            );
            for option in page.dom.elements_named("option") {
                assert!(
                    !output.node_bounds.contains_key(&option.id()),
                    "option escaped picker"
                );
            }
        }
    }
}

#[test]
fn auto_select_width_uses_all_options_and_matches_inline_and_positioned_controls() {
    let mut widths = Vec::new();
    for style in [
        "display:inline-block",
        "display:block",
        "position:absolute",
        "float:left",
    ] {
        let page = Page::parse(
            &format!(
                r#"<style>select{{{style};font:16px/20px sans-serif;padding:0;border:0}}</style>
            <select><option>Short</option><option>A much longer option</option></select>"#
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let control = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(spec) => Some(spec),
                _ => None,
            })
            .unwrap();
        assert!(
            control.rect.width > 180.0 && control.rect.width < 300.0,
            "{:?}",
            control.rect
        );
        widths.push(control.rect.width);
    }
    assert!(
        widths.iter().all(|width| (width - widths[0]).abs() < 0.1),
        "{widths:?}"
    );
}
