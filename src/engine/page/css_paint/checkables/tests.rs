use super::*;
use crate::engine::layout::test_support::FixedMeasurer;
use crate::engine::{CheckableState, layout_page};

fn control(markup: &str) -> (Page, LayoutOutput, Box<ControlSpec>) {
    let page = Page::parse(markup, "https://example.test/");
    let layout = layout_page(&page, 500.0, 300.0, &mut FixedMeasurer);
    let spec = layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) => Some(spec.clone()),
            _ => None,
        })
        .expect("checkable has retained control geometry");
    (page, layout, spec)
}

#[test]
fn checkable_geometry_and_disabled_checked_state_reach_renderer_owned_ink() {
    let (mut page, mut layout, spec) =
        control("<fieldset disabled><label><input type=checkbox checked>Label</label></fieldset>");
    assert_eq!((spec.rect.width, spec.rect.height), (13.0, 13.0));
    assert_eq!(spec.kind, ControlKind::Checkbox);
    assert_eq!(
        spec.checkable.unwrap(),
        CheckableState {
            checked: true,
            indeterminate: false,
            disabled: true,
            appearance: true,
            accent: None,
        }
    );
    assert!(!spec.uses_native_window());
    let bounds = layout.node_bounds.clone();
    page.prepare_css_images(&mut layout);
    assert_eq!(layout.node_bounds, bounds);
    assert!(
        !layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Control(spec) if spec.kind.is_checkable()))
    );
    let key = layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, .. } if url.starts_with(PREFIX) => Some(url),
            _ => None,
        })
        .unwrap();
    assert_eq!((page.images[key].width, page.images[key].height), (13, 13));
}

#[test]
fn changing_checkedness_changes_cache_key_without_losing_dom_bounds() {
    let (mut page, _, spec) =
        control("<input type=checkbox style='width:20px;height:20px;accent-color:red'>");
    let before = descriptor(&DisplayItem::Control(spec.clone())).unwrap();
    let input = page.dom.elements_named("input").next().unwrap();
    input.set_checked(true, true);
    input.set_indeterminate(true);
    let mut layout = layout_page(&page, 500.0, 300.0, &mut FixedMeasurer);
    let updated = layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) => Some(spec),
            _ => None,
        })
        .unwrap();
    assert!(updated.checkable.unwrap().checked && updated.checkable.unwrap().indeterminate);
    assert_ne!(
        descriptor(&DisplayItem::Control(updated.clone())).unwrap(),
        before
    );
    page.prepare_css_images(&mut layout);
    assert_eq!(layout.node_bounds[&input.id()].width, 20.0);
}

#[test]
fn no_appearance_suppresses_widget_ink_but_keeps_author_style_and_activation_geometry() {
    let (mut page, mut layout, spec) = control(
        "<input type=radio checked style='appearance:none;width:24px;height:24px;background:red;border:1px solid blue'>",
    );
    assert!(!spec.checkable.unwrap().appearance);
    assert!(descriptor(&DisplayItem::Control(spec.clone())).is_none());
    assert!(!spec.uses_native_window());
    let items = layout.items.len();
    page.prepare_css_images(&mut layout);
    assert_eq!(layout.items.len(), items);
    assert_eq!(spec.rect.width, 24.0);
}

#[test]
fn checkbox_radio_and_mixed_rasters_are_antialiased_premultiplied_and_distinct() {
    let (_, _, mut spec) =
        control("<input type=checkbox checked style='width:20px;height:20px;accent-color:red'>");
    let checked = raster(&spec, 25, 25).unwrap();
    let mut state = spec.checkable.unwrap();
    state.indeterminate = true;
    spec.checkable = Some(state);
    let mixed = raster(&spec, 25, 25).unwrap();
    spec.kind = ControlKind::Radio;
    state.indeterminate = false;
    spec.checkable = Some(state);
    let radio = raster(&spec, 25, 25).unwrap();
    assert_ne!(checked.bgra, mixed.bgra);
    assert_ne!(checked.bgra, radio.bgra);
    for image in [&checked, &mixed, &radio] {
        assert!(image.bgra.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
        assert!(
            image
                .bgra
                .chunks_exact(4)
                .all(|p| p[..3].iter().all(|channel| *channel <= p[3]))
        );
    }
    assert_eq!(radio.bgra[3], 0);
}
