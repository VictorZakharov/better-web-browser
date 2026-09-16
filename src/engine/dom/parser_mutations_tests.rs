use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};
use html5ever::interface::tree_builder::TreeSink;

#[test]
fn unobserved_and_disconnected_observation_leave_no_parser_journal() {
    let mut parser = HtmlParser::new("<body><script></script><p>text</p>");
    assert!(matches!(parser.advance(), ParserStep::Script(_)));
    assert!(parser.dom().take_parser_mutations().is_empty());
    parser.dom().document.observe_parser_mutations(true);
    parser.dom().document.observe_parser_mutations(false);
    assert!(matches!(parser.advance(), ParserStep::End));
    assert!(parser.dom().take_parser_mutations().is_empty());
}

#[test]
fn static_fragment_parsing_does_not_duplicate_js_mutation_records() {
    let dom = crate::engine::dom::parse("<main></main>");
    dom.document.observe_parser_mutations(true);
    let main = dom.elements_named("main").next().unwrap();
    Node::replace_inner_html(&main, "<b>text</b>", true);
    assert!(dom.take_parser_mutations().is_empty());
}

#[test]
fn parser_reparenting_records_capture_original_and_destination_contexts() {
    let mut parser =
        HtmlParser::new("<body><div><b>one</b><i>two</i></div><section><u>prior</u></section>");
    assert!(matches!(parser.advance(), ParserStep::End));
    let dom = parser.dom();
    dom.document.observe_parser_mutations(true);
    let source = dom.elements_named("div").next().unwrap();
    let destination = dom.elements_named("section").next().unwrap();
    let first = source.children.borrow()[0].clone();
    let second = source.children.borrow()[1].clone();
    dom.reparent_children(&source, &destination);
    let records = dom.take_parser_mutations();
    assert_eq!(records.len(), 4);
    assert_eq!(records[0].removed[0].id(), first.id());
    assert_eq!(records[0].next.as_ref().unwrap().id(), second.id());
    assert_eq!(records[1].previous.as_ref().unwrap().tag_name(), Some("u"));
    assert_eq!(records[2].removed[0].id(), second.id());
    assert!(records[2].previous.is_none());
    assert_eq!(records[3].previous.as_ref().unwrap().id(), first.id());
    assert_eq!(records[0].ancestors[0].id(), source.id());
    assert_eq!(records[1].ancestors[0].id(), destination.id());
}

#[test]
fn parser_existing_body_attributes_and_text_coalescing_have_exact_metadata() {
    let mut parser = HtmlParser::new("<body><script></script><body data-new=value>one");
    assert!(matches!(parser.advance(), ParserStep::Script(_)));
    parser.dom().document.observe_parser_mutations(true);
    assert!(matches!(parser.advance(), ParserStep::End));
    let records = parser.dom().take_parser_mutations();
    let attribute = records.iter().find(|r| r.kind == "attributes").unwrap();
    assert_eq!(attribute.target.tag_name(), Some("body"));
    assert_eq!(
        attribute.attribute,
        Some(("data-new".into(), String::new()))
    );
    assert!(attribute.old_value.is_none());
}
