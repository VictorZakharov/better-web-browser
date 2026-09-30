use super::*;
use better_web_browser::engine::css::Color;

fn spec() -> better_web_browser::engine::ControlSpec {
    better_web_browser::engine::ControlSpec {
        authored_content: false,
        node_id: NodeId::from_wire((1_u128 << 64) | 1).unwrap(),
        rect: Default::default(),
        kind: ControlKind::Text,
        name: String::new(),
        value: "AXB".into(),
        label: String::new(),
        options: Vec::new(),
        selected_index: -1,
        placeholder: String::new(),
        form_id: None,
        background_color: Color::WHITE,
        text_color: Color::BLACK,
        placeholder_color: Color::BLACK,
        border_colors: [Color::TRANSPARENT; 4],
        border_width: [0.0; 4],
        border_radius: 0.0,
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
fn rejected_native_edit_restores_caches_and_allows_following_selection_only_input() {
    unsafe {
        // This is a hidden native EDIT, not a browser: no WS_VISIBLE and no
        // profile or renderer process is created by this regression test.
        let window = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("AXB").as_ptr(),
            WS_POPUP,
            0,
            0,
            100,
            50,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        assert!(!window.is_null());
        let spec = spec();
        let rejection = NativeTextRejection {
            sequence: 5,
            generation: 2,
            target: wire_node(spec.node_id).unwrap(),
            value: "A💡B".into(),
            selection_start: 1,
            selection_end: 3,
        };
        let mut control = page_controls::PageControlWindow {
            window,
            spec,
            brush: null_mut(),
            bubble: null_mut(),
            last_text: "AXB".into(),
            last_selection: (2, 2),
            last_native_selection: (2, 2),
            last_direction: TextSelectionDirection::Backward,
            last_native_input_sequence: 7,
            last_renderer_selection_sequence: 4,
        };
        SendMessageW(window, EM_SETSEL, 2, 2);
        restore_control(&mut control, &rejection);
        assert_eq!(window_text(window), "A💡B");
        assert_eq!(control.last_text, "A💡B");
        assert_eq!(control.spec.value, "A💡B");
        assert_eq!(control.last_selection, (1, 3));
        assert_eq!(control.last_native_selection, (1, 3));
        assert_eq!(control.last_direction, TextSelectionDirection::None);
        // Discarded later native edits must retain their conservative fence.
        assert_eq!(control.last_native_input_sequence, 7);
        assert_eq!(control.last_renderer_selection_sequence, 5);
        assert!(super::super::selection::changed_native_selection(&control).is_none());

        SendMessageW(window, EM_SETSEL, 0, 1);
        // Exercise the exact eligibility helper used by selection-only routing.
        assert_eq!(
            super::super::selection::changed_native_selection(&control),
            Some((0, 1, (0, 1)))
        );
        // PageControlWindow's Drop destroys this hidden HWND.
    }
}
