use super::*;
use std::time::Duration;

#[test]
fn retained_custom_element_app_receives_title_events_after_async_history_navigation() {
    let dom = dom::parse_with_scripting(
        include_str!("../../../../../tests/fixtures/retained-app-title.html"),
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[input]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(dom.title(), "Initial item");

    for (step, name) in [(1, "First item"), (2, "Second item")] {
        let outcome = runtime.advance_time(Duration::from_secs(10), 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(dom.title(), format!("{name} - Retained app"));
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            format!(
                "{step}|{name} - Retained app|/item/{step}|{}|{step}",
                step + 1
            )
        );
        assert_eq!(outcome.history_actions.len(), 1);
        assert_eq!(
            outcome.history_actions[0].url,
            format!("https://example.com/item/{step}")
        );
        assert!(outcome.render_requested);
    }
}
