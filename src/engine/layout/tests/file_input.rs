use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn file_input_projects_hit_testable_control_with_basename_only() {
    let page = Page::parse(
        "<input type=file id=upload name=upload multiple>",
        "https://example.com/",
    );
    let input = page.dom.elements_named("input").next().unwrap();
    let mut measurer = FixedMeasurer;
    let initial = layout_page(&page, 500.0, 300.0, &mut measurer);
    let control = initial
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.node_id == input.id() => Some(control),
            _ => None,
        })
        .expect("file input projects a control");
    assert_eq!(control.kind, ControlKind::File);
    assert!(control.rect.width > 0.0 && control.rect.height > 0.0);
    assert_eq!(control.label, "Choose Files");

    input.set_input_files(vec!["first.txt".into(), "second.txt".into()]);
    let updated = layout_page(&page, 500.0, 300.0, &mut measurer);
    let control = updated
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.node_id == input.id() => Some(control),
            _ => None,
        })
        .expect("selected file input projects a control");
    assert_eq!(control.value, "first.txt (+1)");
    assert_eq!(control.label, "Choose Files  first.txt (+1)");
    assert!(!control.label.contains("fakepath"));
}
