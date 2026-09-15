use super::*;

#[test]
fn storage_event_interface_matches_owned_browser_fixture() {
    let (dom, outcome) = execute_html(include_str!(
        "../../../../benchmarks/alpha/fixtures/storage-event-interface.html"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result = dom
        .elements_named("p")
        .find(|node| node.attr("id").as_deref() == Some("result"))
        .unwrap();
    assert_eq!(
        result.text_content(),
        "26/26 checks passed",
        "{}",
        dom.document.text_content()
    );
}
