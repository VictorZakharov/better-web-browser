use super::*;

#[test]
fn canvas_line_styles_convert_and_iterate_before_updating_state() {
    let source = include_str!("../../../../tests/canvas/line-state.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{source}\ntestCanvasLineState(()=>document.createElement('canvas'));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
