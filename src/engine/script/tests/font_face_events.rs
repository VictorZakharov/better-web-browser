use super::*;

#[test]
fn font_loading_events_have_private_readonly_sequences_and_idl_conversion_order() {
    let source = include_str!("../../../../tests/canvas/font-face-events.js");
    let (dom, outcome) = execute_html(&format!(
        "<output id=probe>pending</output><script>{source}\nprobe.textContent=testFontFaceEvents();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
