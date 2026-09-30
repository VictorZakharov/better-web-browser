use super::*;
use crate::engine::css::{Color, Length};

#[test]
fn colors_and_underline_can_defer_full_paint_without_claiming_item_topology_is_stable() {
    let mut before = ComputedStyle::initial();
    before.display = Display::Block;
    let mut after = before.clone();
    after.color = Color::rgb(12, 34, 56);
    after.background_color = Color::rgb(65, 43, 21);
    after.text_decoration_underline = true;
    assert!(before.deferred_paint_equivalent(&after));
    assert!(after.deferred_paint_equivalent(&before));
    assert!(before.layout_equivalent(&after));
}

#[test]
fn inline_background_presence_changes_do_not_defer_even_when_layout_equivalent() {
    for display in [Display::Inline, Display::Contents] {
        let mut before = ComputedStyle::initial();
        before.display = display;
        let mut after = before.clone();
        after.background_color = Color::WHITE;
        assert!(before.layout_equivalent(&after));
        assert!(!before.deferred_paint_equivalent(&after));
        assert!(!after.deferred_paint_equivalent(&before));
        before.background_color = Color {
            alpha: 128,
            ..Color::BLACK
        };
        assert!(before.deferred_paint_equivalent(&after));
    }
}

#[test]
fn block_and_table_background_presence_changes_are_full_repaint_eligible() {
    for display in [
        Display::Block,
        Display::Table,
        Display::TableRow,
        Display::TableCell,
    ] {
        let mut before = ComputedStyle::initial();
        before.display = display;
        let mut after = before.clone();
        after.background_color = Color::rgb(246, 246, 246);
        assert!(before.deferred_paint_equivalent(&after));
        assert!(after.deferred_paint_equivalent(&before));
    }
}

#[test]
fn geometry_stacking_hit_resources_and_other_paint_changes_fail_closed() {
    let before = ComputedStyle::initial();
    let changes: [fn(&mut ComputedStyle); 12] = [
        |style| style.width = Length::Px(40.0),
        |style| style.font_size = 20.0,
        |style| style.opacity = 0.5,
        |style| style.z_index = Some(3),
        |style| style.visibility = false,
        |style| style.pointer_events = false,
        |style| style.overflow_hidden = true,
        |style| style.background_image = Some("https://example.test/image".into()),
        |style| style.mask_image = Some("https://example.test/mask".into()),
        |style| style.border_radius = Length::Px(2.0),
        |style| style.border_colors[0] = Some(Color::WHITE),
        |style| style.scroll_margin.top = Length::Px(5.0),
    ];
    for (index, change) in changes.iter().enumerate() {
        let mut after = before.clone();
        after.color = Color::WHITE;
        change(&mut after);
        assert!(!before.deferred_paint_equivalent(&after), "change {index}");
        assert!(!after.deferred_paint_equivalent(&before), "change {index}");
    }
}

#[test]
fn changed_custom_properties_are_not_assumed_to_only_affect_color() {
    let before = ComputedStyle::initial();
    let mut after = before.clone();
    std::sync::Arc::make_mut(&mut after.custom_properties).insert("--tone".into(), "blue".into());
    after.color = Color::rgb(0, 0, 255);
    assert!(!before.deferred_paint_equivalent(&after));
}
