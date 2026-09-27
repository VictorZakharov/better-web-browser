use super::*;
use crate::engine::dom::{
    NodeData,
    incremental::{HtmlParser, ParserStep},
    parse,
};

fn first_named(root: &NodeRef, name: &str) -> NodeRef {
    Node::descendants(root)
        .find(|node| node.tag_name() == Some(name))
        .unwrap_or_else(|| panic!("missing {name}"))
}

#[test]
fn navigation_consumes_the_declarative_template_and_attaches_its_contents() {
    let dom = parse(concat!(
        "<div id=host><template shadowrootmode=open shadowrootdelegatesfocus ",
        "shadowrootclonable shadowrootserializable><span>shadow</span></template>",
        "<b>light</b></div>",
    ));
    let host = dom.elements_named("div").next().unwrap();
    let root = host.shadow_root().unwrap();
    let NodeData::ShadowRoot(data) = &root.data else {
        panic!("expected shadow root")
    };
    assert_eq!(data.mode, ShadowRootMode::Open);
    assert!(data.declarative.get());
    assert!(data.delegates_focus && data.clonable && data.serializable);
    assert!(!data.manual_slot_assignment);
    assert_eq!(first_named(&root, "span").text_content(), "shadow");
    assert_eq!(host.children.borrow()[0].tag_name(), Some("b"));
    assert!(!Node::descendants(&dom.document).any(|node| node.tag_name() == Some("span")));
}

#[test]
fn closed_and_nested_roots_keep_their_own_host_and_mode() {
    let dom = parse(concat!(
        "<x-outer><template shadowrootmode=open><x-inner>",
        "<template shadowrootmode=closed><em>nested</em></template>",
        "</x-inner></template></x-outer>",
    ));
    let outer = dom.elements_named("x-outer").next().unwrap();
    let outer_root = outer.shadow_root().unwrap();
    let inner = first_named(&outer_root, "x-inner");
    let inner_root = inner.shadow_root().unwrap();
    assert!(
        matches!(&inner_root.data, NodeData::ShadowRoot(data) if data.mode == ShadowRootMode::Closed)
    );
    assert_eq!(inner_root.shadow_host().unwrap().id(), inner.id());
    assert_eq!(first_named(&inner_root, "em").text_content(), "nested");
}

#[test]
fn valid_non_ascii_and_punctuation_custom_host_names_attach_declarative_roots() {
    let dom = parse(concat!(
        "<x-😍><template shadowrootmode=open><b>emoji</b></template></x-😍>",
        "<x-@><template shadowrootmode=closed><i>punctuation</i></template></x-@>",
    ));
    let emoji = dom.elements_named("x-😍").next().unwrap();
    let punctuation = dom.elements_named("x-@").next().unwrap();
    assert_eq!(
        first_named(&emoji.shadow_root().unwrap(), "b").text_content(),
        "emoji"
    );
    assert!(matches!(&punctuation.shadow_root().unwrap().data,
        NodeData::ShadowRoot(data) if data.mode == ShadowRootMode::Closed));
    assert_eq!(
        first_named(&punctuation.shadow_root().unwrap(), "i").text_content(),
        "punctuation"
    );
}

#[test]
fn parser_reads_manual_slot_assignment_enum() {
    let dom = parse(
        "<x-card><template shadowrootmode=open shadowrootslotassignment=MaNuAl><slot></slot></template></x-card>",
    );
    let host = dom.elements_named("x-card").next().unwrap();
    assert!(
        matches!(&host.shadow_root().unwrap().data, NodeData::ShadowRoot(data) if data.manual_slot_assignment)
    );
}

#[test]
fn declarative_registry_marker_starts_with_null_registry_and_is_not_copied_as_markup() {
    let dom = parse(concat!(
        "<x-scoped><template shadowrootmode=open shadowrootcustomelementregistry>",
        "<x-child></x-child></template></x-scoped>",
        "<x-global><template shadowrootmode=open><x-child></x-child></template></x-global>",
    ));
    let scoped = dom.elements_named("x-scoped").next().unwrap();
    let global = dom.elements_named("x-global").next().unwrap();
    let scoped_root = scoped.shadow_root().unwrap();
    let global_root = global.shadow_root().unwrap();
    assert!(matches!(&scoped_root.data, NodeData::ShadowRoot(data)
        if data.declarative.get() && data.keep_registry_null.get() && !data.registry_is_global.get()));
    assert!(matches!(&global_root.data, NodeData::ShadowRoot(data)
        if data.declarative.get() && !data.keep_registry_null.get() && data.registry_is_global.get()));
    assert!(scoped.children.borrow().is_empty());

    // DOM attach-shadow returns before replacing a reused declarative root's
    // registry, even when the new options would have chosen the global one.
    let reused = Node::attach_shadow(&scoped, ShadowRootMode::Open, false, false, false).unwrap();
    assert_eq!(reused.id(), scoped_root.id());
    assert!(matches!(&reused.data, NodeData::ShadowRoot(data)
        if !data.declarative.get() && data.keep_registry_null.get() && !data.registry_is_global.get()));
}

#[test]
fn shadowrootmode_is_ascii_case_insensitive_without_rewriting_fallback_markup() {
    let dom = parse(concat!(
        "<x-open><template shadowrootmode=OPEN><b>open</b></template></x-open>",
        "<x-closed><template shadowrootmode=Closed><i>closed</i></template></x-closed>",
        "<button><template shadowrootmode=ClOsEd><em>fallback</em></template></button>",
    ));
    let open = dom.elements_named("x-open").next().unwrap();
    let closed = dom.elements_named("x-closed").next().unwrap();
    assert!(
        matches!(&open.shadow_root().unwrap().data, NodeData::ShadowRoot(data) if data.mode == ShadowRootMode::Open)
    );
    assert!(
        matches!(&closed.shadow_root().unwrap().data, NodeData::ShadowRoot(data) if data.mode == ShadowRootMode::Closed)
    );
    assert_eq!(
        first_named(&open.shadow_root().unwrap(), "b").text_content(),
        "open"
    );
    assert_eq!(
        first_named(&closed.shadow_root().unwrap(), "i").text_content(),
        "closed"
    );
    let button = dom.elements_named("button").next().unwrap();
    assert!(button.shadow_root().is_none());
    assert_eq!(
        button.children.borrow()[0]
            .attr("shadowrootmode")
            .as_deref(),
        Some("ClOsEd")
    );

    let mut parser = HtmlParser::streaming();
    parser
        .append("<x-card><template shadowrootmode=\"Cl", false)
        .unwrap();
    assert!(matches!(parser.advance(), ParserStep::NeedInput));
    parser
        .append("OsEd\"><span>stream</span></template></x-card>", true)
        .unwrap();
    assert!(matches!(parser.advance(), ParserStep::End));
    let host = parser.dom().elements_named("x-card").next().unwrap();
    assert!(
        matches!(&host.shadow_root().unwrap().data, NodeData::ShadowRoot(data) if data.mode == ShadowRootMode::Closed)
    );
}

#[test]
fn invalid_hosts_duplicate_roots_and_inert_template_contents_remain_templates() {
    let dom = parse(concat!(
        "<button><template shadowrootmode=open><i>button</i></template></button>",
        "<div><template shadowrootmode=open><i>first</i></template>",
        "<template shadowrootmode=closed><i>second</i></template></div>",
        "<template id=outer><x-card><template shadowrootmode=open>",
        "<i>inert</i></template></x-card></template>",
    ));
    let button = dom.elements_named("button").next().unwrap();
    assert!(button.shadow_root().is_none());
    assert_eq!(button.children.borrow()[0].tag_name(), Some("template"));
    let div = dom.elements_named("div").next().unwrap();
    assert!(div.shadow_root().is_some());
    assert_eq!(div.children.borrow()[0].tag_name(), Some("template"));
    let outer = dom
        .elements_named("template")
        .find(|node| node.attr("id").as_deref() == Some("outer"))
        .unwrap();
    let contents = outer
        .element()
        .unwrap()
        .template_contents
        .borrow()
        .clone()
        .unwrap();
    let inert_host = first_named(&contents, "x-card");
    assert!(inert_host.shadow_root().is_none());
    assert_eq!(inert_host.children.borrow()[0].tag_name(), Some("template"));
}

#[test]
fn fragment_and_detached_document_parsers_do_not_opt_in() {
    let markup = "<template shadowrootmode=open><span>plain</span></template>";
    let host = Node::create_element("div");
    Node::replace_inner_html(&host, markup, true);
    assert!(host.shadow_root().is_none());
    assert_eq!(host.children.borrow()[0].tag_name(), Some("template"));

    let detached = super::super::parse_detached_html(&format!("<div>{markup}</div>"));
    let detached_host = detached.elements_named("div").next().unwrap();
    assert!(detached_host.shadow_root().is_none());
    assert_eq!(
        detached_host.children.borrow()[0].tag_name(),
        Some("template")
    );
}

#[test]
fn detached_unsafe_html_parser_opts_in_but_starts_with_a_null_registry() {
    let dom = super::super::parse_detached_html_with_shadow_roots(concat!(
        "<!doctype html><x-card><template shadowrootmode=open shadowrootserializable>",
        "<span>detached</span></template></x-card>",
    ));
    let host = dom.elements_named("x-card").next().unwrap();
    let root = host.shadow_root().unwrap();
    assert!(matches!(&root.data, NodeData::ShadowRoot(data)
        if data.declarative.get() && data.serializable && data.registry_is_null.get()
            && !data.registry_is_global.get() && !data.keep_registry_null.get()));
    assert_eq!(first_named(&root, "span").text_content(), "detached");
}

#[test]
fn streaming_chunks_attach_before_contents_arrive() {
    let mut parser = HtmlParser::streaming();
    parser.append("<x-card><tem", false).unwrap();
    assert!(matches!(parser.advance(), ParserStep::NeedInput));
    parser
        .append("plate shadowrootmode=\"open\"><span>", false)
        .unwrap();
    assert!(matches!(parser.advance(), ParserStep::NeedInput));
    let host = parser.dom().elements_named("x-card").next().unwrap();
    assert!(host.shadow_root().is_some());
    parser
        .append("streamed</span></template></x-card>", true)
        .unwrap();
    assert!(matches!(parser.advance(), ParserStep::End));
    let root = host.shadow_root().unwrap();
    assert_eq!(first_named(&root, "span").text_content(), "streamed");
    assert!(host.children.borrow().is_empty());
}

#[test]
fn matching_attach_shadow_reuses_and_empties_only_declarative_roots() {
    let dom = parse("<div><template shadowrootmode=closed><b>old</b></template></div>");
    let host = dom.elements_named("div").next().unwrap();
    let original = host.shadow_root().unwrap();
    assert!(Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).is_none());
    assert!(original.children.borrow().len() == 1);
    let reused = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    assert_eq!(reused.id(), original.id());
    assert!(reused.children.borrow().is_empty());
    assert!(matches!(&reused.data, NodeData::ShadowRoot(data) if !data.declarative.get()));
    assert!(Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).is_none());
}
