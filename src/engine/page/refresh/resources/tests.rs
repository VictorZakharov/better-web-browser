use crate::engine::dom::{Node, NodeRef};
use crate::engine::invalidation::{
    AttributeChange, MutationKind, RenderInvalidation, changed_class_tokens,
};
use crate::engine::page::{Page, PageResource};
use crate::limits::MAX_STYLE_IMAGES;

const CSS: &str = "<style>.hidden{display:none} .icon{background-image:url(icon.png)} \
    .shown{background-image:url(shown.png)}</style>";

fn page(body: &str) -> Page {
    let mut page = Page::parse(&format!("{CSS}{body}"), "https://example.test/");
    page.refresh_resources(800.0);
    page
}

fn by_id(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .expect("element")
}

fn has_image(page: &Page, name: &str) -> bool {
    page.resources.contains(&PageResource::Image {
        url: format!("https://example.test/{name}"),
    })
}

fn set_class(page: &mut Page, id: &str, class: &str) {
    let target = by_id(page, id);
    let previous = target.attr("class").unwrap_or_default();
    target.set_attr("class", class);
    let invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("class").impact(),
        mutation_count: 1,
        attribute_changes: vec![AttributeChange {
            node: target.id(),
            tokens: changed_class_tokens(&previous, class).expect("tokens changed"),
            attributes: Vec::new(),
        }],
        ..RenderInvalidation::default()
    };
    page.refresh_resources_after_invalidation(800.0, &invalidation);
}

#[test]
fn incremental_refreshes_discover_images_of_changed_styles() {
    let mut page = page("<main><div id=target class=hidden></div><p id=list></p></main>");
    assert!(!has_image(&page, "shown.png"));
    set_class(&mut page, "target", "shown");
    assert!(has_image(&page, "shown.png"));

    let list = by_id(&page, "list");
    let child = Node::create_element_for(&page.dom.document, "span");
    child.set_attr("class", "icon");
    Node::append_child(&list, child);
    let invalidation = RenderInvalidation {
        impact: MutationKind::ChildList.impact(),
        mutation_count: 1,
        roots: vec![list.id()],
        child_list_roots: vec![list.id()],
        element_roots: Vec::new(),
        ..RenderInvalidation::default()
    };
    page.refresh_resources_after_invalidation(800.0, &invalidation);
    assert!(has_image(&page, "icon.png"));
}

#[test]
fn images_beyond_the_per_pass_limit_are_found_by_the_next_pass() {
    let count = MAX_STYLE_IMAGES + 8;
    let rules = (0..count)
        .map(|index| format!(".i{index}{{background-image:url(i{index}.png)}}"))
        .collect::<String>();
    let elements = (0..count)
        .map(|index| format!("<b class=i{index}></b>"))
        .collect::<String>();
    let mut page = page(&format!("<style>{rules}</style>{elements}<p id=other></p>"));
    let found = |page: &Page| {
        (0..count)
            .filter(|i| has_image(page, &format!("i{i}.png")))
            .count()
    };
    assert_eq!(found(&page), MAX_STYLE_IMAGES);
    // An unrelated change restyles one element; the remainder still arrives.
    set_class(&mut page, "other", "x");
    assert_eq!(found(&page), count);
}

#[test]
fn a_refresh_after_deferred_styles_restyles_the_whole_document() {
    let mut page = page("<div id=a class=icon></div>");
    // Rendering checkpoints defer the cascade while stylesheets block rendering; the DOM keeps
    // changing in between, and the next refresh is scoped to an unrelated element.
    page.refresh_resources_deferring_styles(800.0, 800.0);
    let body = page.dom.elements_named("body").next().expect("body");
    let added = Node::create_element_for(&page.dom.document, "p");
    added.set_attr("class", "shown");
    Node::append_child(&body, added.clone());
    page.refresh_resources_deferring_styles(800.0, 800.0);
    assert!(page.cached_style(800.0).is_none());
    assert!(
        !has_image(&page, "shown.png"),
        "style images wait for the cascade"
    );

    let target = by_id(&page, "a");
    let stats = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            impact: MutationKind::Attribute("title").impact(),
            mutation_count: 1,
            roots: vec![target.id()],
            ..RenderInvalidation::default()
        },
    );
    assert!(stats.recomputed_styles >= stats.total_styles, "{stats:?}");
    let fresh = page.style(800.0);
    let refreshed = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(refreshed.styles.get(&node.id()) == fresh.styles.get(&node.id()));
    }
    assert!(refreshed.styles.contains_key(&added.id()));
    assert!(has_image(&page, "shown.png"));
}

#[test]
fn a_deferred_invalidation_reaches_whichever_refresh_comes_next() {
    let mut page = page(&"<div><i>x</i></div>".repeat(30));
    let body = page.dom.elements_named("body").next().expect("body");
    let added = Node::create_element_for(&page.dom.document, "p");
    added.set_attr("class", "shown");
    Node::append_child(&body, added.clone());
    // A rendering checkpoint skipped its frame after this insertion.
    page.defer_style_invalidation(
        800.0,
        800.0,
        &RenderInvalidation {
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            roots: vec![body.id()],
            child_list_roots: vec![body.id()],
            element_roots: Vec::new(),
            ..RenderInvalidation::default()
        },
    );
    // A layout flush computes only part of each style, so the work stays pending.
    page.refresh_layout_styles_after_invalidation_for_viewport(
        800.0,
        800.0,
        &RenderInvalidation::default(),
    );
    assert!(page.has_deferred_style_invalidation());

    // A refresh for an unrelated reason (a viewport change, say) styles the insertion too,
    // without restyling the whole document.
    let stats = page.refresh_resources_after_invalidation(800.0, &RenderInvalidation::default());
    assert!(!page.has_deferred_style_invalidation());
    assert!(
        stats.recomputed_styles < stats.total_styles / 2,
        "{stats:?}"
    );
    let fresh = page.style(800.0);
    let refreshed = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(refreshed.styles.get(&node.id()) == fresh.styles.get(&node.id()));
    }
    assert!(has_image(&page, "shown.png"));
}

#[test]
fn a_stylesheet_arriving_after_the_first_frame_restyles_only_its_rule_subjects() {
    let mut page = page(&format!(
        "<link rel=stylesheet href=late.css><p class=late>late</p>{}",
        "<div><i>x</i></div>".repeat(40)
    ));
    page.add_linked_stylesheet_response(
        "https://example.test/late.css",
        "https://example.test/late.css",
        ".late{color:red}".into(),
    );
    let stats = page.refresh_resources_after_invalidation(800.0, &RenderInvalidation::default());
    assert!(!stats.full_rebuild, "{stats:?}");
    assert!(
        stats.recomputed_styles < stats.total_styles / 4,
        "{stats:?}"
    );
    let fresh = page.style(800.0);
    let refreshed = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(refreshed.styles.get(&node.id()) == fresh.styles.get(&node.id()));
    }
}
