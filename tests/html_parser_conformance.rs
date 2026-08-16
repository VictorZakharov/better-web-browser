mod html_parser_support;

use better_web_browser::engine::dom::{Node, parse_with_scripting};
use better_web_browser::limits::MAX_DOM_DEPTH;
use html_parser_support::{Fixture, parse_fixtures, serialize_document, serialize_fragment};

const WPT_DOCUMENT_FIXTURES: &str = include_str!("html-parser/fixtures/wpt-documents.dat");
const WPT_FRAGMENT_FIXTURES: &str = include_str!("html-parser/fixtures/wpt-fragments.dat");
const LOCAL_FIXTURES: &str = include_str!("html-parser/fixtures/local-regressions.dat");

#[test]
fn curated_wpt_documents_match_the_engine_dom() {
    run_fixtures("wpt-documents.dat", WPT_DOCUMENT_FIXTURES, false);
}

#[test]
fn curated_wpt_fragments_use_the_target_element_context() {
    run_fixtures("wpt-fragments.dat", WPT_FRAGMENT_FIXTURES, true);
}

#[test]
fn standards_derived_parser_regressions_match_the_engine_dom() {
    run_fixtures("local-regressions.dat", LOCAL_FIXTURES, false);
}

#[test]
fn deeply_nested_malformed_input_remains_finite_and_acyclic() {
    let mut input = "<main>".to_string();
    input.push_str(&"<div>".repeat(2_048));
    input.push_str("end");
    input.push_str(&"</span>".repeat(2_048));

    let dom = parse_with_scripting(&input, true);
    let serialized = serialize_document(&dom.document)
        .expect("deep malformed document should remain a finite tree");

    assert!(!serialized.contains("\"end\""));
    assert!(serialized.lines().count() <= MAX_DOM_DEPTH + 8);
    assert!(
        dom.errors
            .borrow()
            .iter()
            .any(|error| error.starts_with("safety limit:"))
    );
}

fn run_fixtures(suite: &str, source: &str, fragments_only: bool) {
    let fixtures = parse_fixtures(source).unwrap_or_else(|error| panic!("{suite}: {error}"));
    assert!(!fixtures.is_empty(), "{suite} must contain fixtures");

    for (index, fixture) in fixtures.iter().enumerate() {
        assert_eq!(
            fixture.context.is_some(),
            fragments_only,
            "{suite} case {} has the wrong parsing mode",
            index + 1
        );
        for scripting_enabled in fixture.scripting_modes() {
            let actual = render_fixture(fixture, scripting_enabled).unwrap_or_else(|error| {
                panic!(
                    "{suite} case {} (scripting={scripting_enabled}) violated a DOM invariant: {error}",
                    index + 1
                )
            });
            assert_eq!(
                actual,
                fixture.expected,
                "{suite} case {} (scripting={scripting_enabled}) failed for input {:?}",
                index + 1,
                fixture.input
            );
        }
    }
}

fn render_fixture(fixture: &Fixture, scripting_enabled: bool) -> Result<String, String> {
    let Some(context) = &fixture.context else {
        let dom = parse_with_scripting(&fixture.input, scripting_enabled);
        return serialize_document(&dom.document);
    };

    let context_node = context.create_node();
    Node::replace_inner_html(&context_node, &fixture.input, scripting_enabled);
    serialize_fragment(&context_node)
}
