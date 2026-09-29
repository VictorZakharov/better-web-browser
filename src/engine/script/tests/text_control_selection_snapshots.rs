use super::*;

#[test]
fn selection_mirrors_snapshot_private_values_and_normalize_textarea_newlines() {
    let (dom, outcome) = execute_html(
        r#"<body><input value=abcdef><textarea></textarea><output></output><script>
            const input = document.querySelector('input');
            input.setRangeText('XY', 1, 3, 'select');
            Object.defineProperty(input, 'value', {get: () => 'author-installed getter'});
            input.setSelectionRange(1, 3);
            const area = document.querySelector('textarea');
            area.value = 'A\r\n💡B';
            area.setSelectionRange(2, 4);
            document.querySelector('output').textContent = input.value;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "author-installed getter"
    );
    let input_id = dom.elements_named("input").next().unwrap().id();
    let area_id = dom.elements_named("textarea").next().unwrap().id();
    let input = outcome
        .selection_actions
        .iter()
        .find(|action| action.node == input_id)
        .unwrap();
    let area = outcome
        .selection_actions
        .iter()
        .find(|action| action.node == area_id)
        .unwrap();
    assert_eq!(input.value, "aXYdef");
    assert_eq!((input.selection_start, input.selection_end), (1, 3));
    assert_eq!(area.value, "A\n💡B");
    assert_eq!((area.selection_start, area.selection_end), (2, 4));
}

#[test]
fn native_selection_snapshots_share_a_bounded_budget_without_restricting_dom_values() {
    let (dom, outcome) = execute_html(
        r#"<body><input id=first><input id=second><output></output><script>
            const first = document.getElementById('first');
            const second = document.getElementById('second');
            first.value = 'x'.repeat(40000);
            first.setSelectionRange(1, 2);
            second.value = 'y'.repeat(40000);
            second.setSelectionRange(2, 3);
            first.value = 'z'.repeat(70000);
            document.querySelector('output').textContent = JSON.stringify([
                first.value.length, first.selectionStart, first.selectionEnd,
                second.selectionStart, second.selectionEnd]);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "[70000,70000,70000,2,3]"
    );
    assert_eq!(outcome.selection_actions.len(), 1);
    let action = &outcome.selection_actions[0];
    assert_eq!(
        action.node,
        dom.elements_named("input")
            .find(|node| node.attr("id").as_deref() == Some("second"))
            .unwrap()
            .id()
    );
    assert_eq!(action.value.len(), 40000);
    assert_eq!((action.selection_start, action.selection_end), (2, 3));
    assert!(
        outcome
            .selection_actions
            .iter()
            .map(|action| action.value.len())
            .sum::<usize>()
            <= crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES
    );
}
