use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};

#[test]
fn only_parsers_that_opt_in_retain_render_changes() {
    let mut parser = HtmlParser::new("<body><p>text</p>");
    assert!(matches!(parser.advance(), ParserStep::End));
    assert!(parser.dom().take_parser_render_changes().is_empty());
}

#[test]
fn records_appends_with_previous_parent_emptiness() {
    let mut parser = HtmlParser::streaming();
    parser.dom().retain_parser_render_changes();
    parser.append("<body><div> <!-- c -->", false).unwrap();
    assert!(matches!(parser.advance(), ParserStep::NeedInput));
    parser.dom().take_parser_render_changes();
    parser.append("<p>one</p><p>two", true).unwrap();
    assert!(matches!(parser.advance(), ParserStep::End));
    let changes = parser.dom().take_parser_render_changes();
    let inserted = changes
        .iter()
        .filter_map(|change| match change {
            ParserRenderChange::Inserted {
                parent,
                child,
                appended,
                parent_was_empty,
            } => Some((
                parent.tag_name(),
                child.tag_name(),
                *appended,
                *parent_was_empty,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        inserted,
        vec![
            (Some("div"), Some("p"), true, true),
            (Some("p"), None, true, true),
            (Some("div"), Some("p"), true, false),
            (Some("p"), None, true, true),
        ]
    );
    assert!(parser.dom().take_parser_render_changes().is_empty());
}

#[test]
fn records_text_appends_and_parser_moves() {
    let mut parser = HtmlParser::streaming();
    parser.dom().retain_parser_render_changes();
    parser.append("<body><p>one", false).unwrap();
    assert!(matches!(parser.advance(), ParserStep::NeedInput));
    parser.dom().take_parser_render_changes();
    parser.append(" two</p><table><b>moved</b>", true).unwrap();
    assert!(matches!(parser.advance(), ParserStep::End));
    let changes = parser.dom().take_parser_render_changes();
    assert!(changes.iter().any(|change| matches!(
        change,
        ParserRenderChange::Text { node, parent_was_empty: false }
            if node.text_content() == "one two"
    )));
    // Foster parenting inserts the <b> before the table rather than appending it.
    assert!(changes.iter().any(|change| matches!(
        change,
        ParserRenderChange::Inserted { child, appended: false, .. }
            if child.tag_name() == Some("b")
    )));
}
