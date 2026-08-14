use super::*;
use html5ever::{expanded_name, local_name, ns};

#[test]
fn html5_tree_builder_repairs_structure() {
    let dom = parse("<title>Test &amp; Repair</title><p>one<p>two");
    assert_eq!(dom.title(), "Test & Repair");
    let paragraphs = dom.elements_named("p").collect::<Vec<_>>();
    assert_eq!(paragraphs.len(), 2);
    assert_eq!(paragraphs[0].text_content(), "one");
    assert_eq!(paragraphs[1].text_content(), "two");
}

#[test]
fn exposes_attributes_and_classes() {
    let dom = parse(r#"<main id="app" class="page centered">Hello</main>"#);
    let main = dom.elements_named("main").next().unwrap();
    assert_eq!(main.attr("id").as_deref(), Some("app"));
    assert!(main.has_class("centered"));
    assert!(!main.has_class("center"));
}

#[test]
fn handles_foster_parenting_without_cycles() {
    let dom = parse("<table>outside<tr><td>inside</table>");
    assert_eq!(
        dom.elements_named("td").next().unwrap().text_content(),
        "inside"
    );
    assert!(dom.document.text_content().contains("outside"));
}

#[test]
fn recognizes_html_namespace() {
    let dom = parse("<svg><foreignObject><p>html</p></foreignObject></svg>");
    let paragraph = dom.elements_named("p").next().unwrap();
    assert_eq!(
        paragraph.element().unwrap().name.expanded(),
        expanded_name!(html "p")
    );
    assert_eq!(paragraph.element().unwrap().name.local, local_name!("p"));
}

#[test]
fn parses_noscript_content_when_scripting_is_unavailable() {
    let dom = parse("<body><noscript><p>Script-free fallback</p></noscript></body>");
    let fallback = dom.elements_named("p").next().unwrap();
    assert_eq!(fallback.text_content(), "Script-free fallback");
}

#[test]
fn exposes_dom_mutations_needed_by_script_bindings() {
    let dom = parse("<main id=app><span>old</span></main>");
    let main = dom.elements_named("main").next().unwrap();
    let paragraph = Node::create_element_for(&main, "p");
    paragraph.set_attr("class", "message");
    Node::set_text_content(&paragraph, "new");
    assert!(Node::append_child(&main, paragraph.clone()));
    assert_eq!(paragraph.parent().unwrap().tag_name(), Some("main"));
    assert_eq!(paragraph.attr("class").as_deref(), Some("message"));
    assert_eq!(main.text_content(), "oldnew");

    Node::replace_inner_html(&main, "<strong>replaced</strong>", true);
    assert_eq!(main.text_content(), "replaced");
    assert_eq!(main.children.borrow()[0].tag_name(), Some("strong"));
}

#[test]
fn parses_inner_html_in_the_target_elements_context() {
    let table = Node::create_element("table");
    Node::replace_inner_html(&table, "<col><tbody><tr><td>cell", true);

    let children = table.children.borrow();
    assert_eq!(children[0].tag_name(), Some("colgroup"));
    assert_eq!(children[0].children.borrow()[0].tag_name(), Some("col"));
    assert_eq!(children[1].tag_name(), Some("tbody"));
    assert_eq!(table.text_content(), "cell");
}

#[test]
fn fragment_parser_closes_paragraphs_for_sectioning_content() {
    let container = Node::create_element("div");
    Node::replace_inner_html(&container, "<p>before<section>inside</section>", true);

    let children = container.children.borrow();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].tag_name(), Some("p"));
    assert_eq!(children[1].tag_name(), Some("section"));
}

#[test]
fn fragment_parser_preserves_foreign_content_namespaces() {
    let container = Node::create_element("div");
    Node::replace_inner_html(
        &container,
        "<svg><circle></circle></svg><math><mi>x</mi></math>",
        true,
    );

    let children = container.children.borrow();
    assert_eq!(
        children[0].namespace_uri(),
        Some("http://www.w3.org/2000/svg")
    );
    assert_eq!(
        children[1].namespace_uri(),
        Some("http://www.w3.org/1998/Math/MathML")
    );
}

#[test]
fn stable_identity_survives_detach_and_reinsertion_without_aliasing() {
    let dom = parse("<main><span>original</span></main>");
    let main = dom.elements_named("main").next().unwrap();
    let original = dom.elements_named("span").next().unwrap();
    let original_id = original.id();

    assert!(Node::remove_child(&main, &original));
    assert!(dom.find_node(original_id).is_none());

    let replacement = Node::create_element_for(&dom.document, "span");
    assert_ne!(replacement.id(), original_id);
    assert_eq!(replacement.id().document(), original_id.document());

    assert!(Node::append_child(&main, original.clone()));
    assert_eq!(original.id(), original_id);
    assert_eq!(dom.find_node(original_id).unwrap().id(), original_id);
}

#[test]
fn replacement_documents_use_distinct_serializable_namespaces() {
    let first = parse("<p>first</p>");
    let second = parse("<p>second</p>");
    let first_id = first.elements_named("p").next().unwrap().id();
    let second_id = second.elements_named("p").next().unwrap().id();

    assert_ne!(first_id.document(), second_id.document());
    assert_ne!(first_id.to_wire(), second_id.to_wire());
    assert_eq!(NodeId::from_wire(first_id.to_wire()), Some(first_id));
    assert_eq!(NodeId::from_wire(second_id.to_wire()), Some(second_id));
    assert_eq!(NodeId::from_wire(0), None);
    assert!(first.find_node(second_id).is_none());
    assert!(second.find_node(first_id).is_none());
}

#[test]
fn rejects_cross_document_insertion_until_explicit_adoption_exists() {
    let first = parse("<main></main>");
    let second = parse("<span>foreign</span>");
    let main = first.elements_named("main").next().unwrap();
    let foreign = second.elements_named("span").next().unwrap();
    let foreign_id = foreign.id();

    assert!(!Node::append_child(&main, foreign.clone()));
    assert!(foreign.parent().is_some());
    assert_eq!(foreign.id(), foreign_id);
    assert!(first.find_node(foreign_id).is_none());
    assert!(second.find_node(foreign_id).is_some());
}

#[test]
fn fragment_nodes_inherit_the_target_document_identity() {
    let dom = parse("<main></main>");
    let main = dom.elements_named("main").next().unwrap();
    Node::replace_inner_html(&main, "<strong>fragment</strong>", true);
    let strong = dom.elements_named("strong").next().unwrap();

    assert_eq!(strong.id().document(), main.id().document());
    assert!(dom.find_node(strong.id()).is_some());
}

#[test]
fn mutation_versions_track_document_and_affected_subtrees() {
    let dom = parse("<main><section><span>text</span></section></main>");
    let main = dom.elements_named("main").next().unwrap();
    let section = dom.elements_named("section").next().unwrap();
    let span = dom.elements_named("span").next().unwrap();
    let initial_document_version = dom.mutation_version();

    span.set_attr("data-state", "changed");
    let attribute_version = dom.mutation_version();
    assert!(attribute_version > initial_document_version);
    assert_eq!(span.subtree_mutation_version(), attribute_version);
    assert_eq!(section.subtree_mutation_version(), attribute_version);
    assert_eq!(main.subtree_mutation_version(), attribute_version);

    assert!(Node::remove_child(&section, &span));
    let removal_version = dom.mutation_version();
    assert!(removal_version > attribute_version);
    assert_eq!(section.subtree_mutation_version(), removal_version);
    assert_eq!(main.subtree_mutation_version(), removal_version);
    assert_eq!(span.subtree_mutation_version(), attribute_version);

    assert!(Node::append_child(&section, span.clone()));
    let reinsertion_version = dom.mutation_version();
    assert!(reinsertion_version > removal_version);
    assert_eq!(section.subtree_mutation_version(), reinsertion_version);
    assert_eq!(span.id(), dom.elements_named("span").next().unwrap().id());
}
