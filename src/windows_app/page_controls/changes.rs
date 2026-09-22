//! A CSS restyle must update the native control surface as well as page paint.

use super::*;

pub(in crate::windows_app) fn native_controls_changed(
    old: &LayoutOutput,
    new: &LayoutOutput,
) -> bool {
    let mut old = old.items.iter().filter_map(native_control);
    let mut new = new.items.iter().filter_map(native_control);
    loop {
        match (old.next(), new.next()) {
            (None, None) => return false,
            (Some(before), Some(after)) if same_native_control(before, after) => {}
            _ => return true,
        }
    }
}

fn native_control(item: &DisplayItem) -> Option<&better_web_browser::engine::ControlSpec> {
    match item {
        DisplayItem::Control(spec) if !spec.authored_content => Some(spec),
        _ => None,
    }
}

fn same_native_control(
    before: &better_web_browser::engine::ControlSpec,
    after: &better_web_browser::engine::ControlSpec,
) -> bool {
    // Position is synchronized without destroying the HWND. The native EDIT already
    // owns live text and selection; a DOM value update must not replace it mid-edit.
    let mut before = before.clone();
    before.rect = after.rect;
    before.value.clone_from(&after.value);
    before == *after
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::engine::css::Color;

    fn control() -> better_web_browser::engine::ControlSpec {
        better_web_browser::engine::ControlSpec {
            authored_content: false,
            node_id: better_web_browser::engine::dom::NodeId::from_wire((1_u128 << 64) | 1)
                .unwrap(),
            rect: Default::default(),
            kind: ControlKind::Search,
            name: String::new(),
            value: "test".into(),
            label: String::new(),
            options: Vec::new(),
            selected_index: -1,
            placeholder: String::new(),
            form_id: None,
            background_color: Color::rgb(51, 51, 51),
            text_color: Color::WHITE,
            placeholder_color: Color::WHITE,
            border_colors: [Color::TRANSPARENT; 4],
            border_width: [0.0; 4],
            border_radius: 20.0,
            padding: [0.0; 4],
            font: FontSpec {
                family: "Arial".into(),
                size: 16.0,
                weight: 400,
                italic: false,
                underline: false,
                letter_spacing: 0.0,
                word_spacing: 0.0,
            },
            icon_url: None,
            icon_width: 0.0,
            icon_height: 0.0,
            invalid: false,
            validation_message: String::new(),
        }
    }

    #[test]
    fn geometry_and_live_value_preserve_the_edit_but_restyle_rebuilds_it() {
        let old = control();
        let mut updated = old.clone();
        updated.rect.x = 24.0;
        updated.value = "typed".into();
        assert!(same_native_control(&old, &updated));
        updated.background_color = Color::rgb(35, 35, 35);
        assert!(!same_native_control(&old, &updated));
        updated.background_color = old.background_color;
        updated.font.size = 18.0;
        assert!(!same_native_control(&old, &updated));
    }
}
