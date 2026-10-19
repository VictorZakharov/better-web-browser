use crate::engine::dom::{Node, NodeRef};
use crate::engine::invalidation::{
    ClassChange, MutationKind, RenderInvalidation, changed_class_tokens,
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
        class_changes: vec![ClassChange {
            node: target.id(),
            tokens: changed_class_tokens(&previous, class).expect("tokens changed"),
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
