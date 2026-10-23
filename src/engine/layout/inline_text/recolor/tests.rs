use super::*;
use crate::engine::invalidation::{AttributeChange, MutationKind, changed_class_tokens};
use crate::engine::layout::test_support::FixedMeasurer;

fn page(css: &str, body: &str) -> Page {
    let mut page = Page::parse(
        &format!(
            "<!doctype html><style>body{{margin:0}}{css}</style><main id=target>{body}</main>"
        ),
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800., 600.);
    page
}

fn node(page: &Page, id: &str) -> NodeRef {
    Node::composed_descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

/// Sets `#target`'s class, refreshes styles as a rendering checkpoint does, and tries the
/// patch. Returns whether it applied; when it did, the items must equal a fresh layout's.
fn recolor(page: &mut Page, retained: &mut LayoutOutput, class: &str) -> bool {
    let target = node(page, "target");
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
    let stats = page.refresh_resources_after_invalidation_for_viewport(800., 600., &invalidation);
    let fresh = layout_page(page, 800., 600., &mut FixedMeasurer);
    let applied = retained.try_refresh_text_color(page, [800., 600., 800.], &invalidation, &stats);
    if applied {
        assert_eq!(
            retained.items, fresh.items,
            "patched items differ after class {class:?}"
        );
        assert_eq!(retained.node_bounds, fresh.node_bounds);
    }
    *retained = fresh;
    applied
}

const TEXT: &str = "<p>plain <b>bold</b> <a href=/x>link <i>nested</i></a></p><p><a href=/y>only a link</a></p>\
    <p style='text-decoration:underline'>underlined</p><ul><li>bullet</li></ul>";

#[test]
fn class_changes_that_only_recolor_text_repaint_without_layout() {
    for css in [
        ".dark p{color:#333}",
        ".dark{color:#333}",
        ".dark a{color:#36c} .dark b{color:red}",
        ".dark li{color:green}",
    ] {
        let mut page = page(css, TEXT);
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        assert!(recolor(&mut page, &mut retained, "dark"), "{css}");
        assert!(recolor(&mut page, &mut retained, "light"), "{css} back");
    }
}

#[test]
fn colors_painted_outside_text_take_normal_layout() {
    for (css, body) in [
        (".dark{color:red} p{border:1px solid}", TEXT),
        (".dark{color:red} p{box-shadow:1px 1px 2px}", TEXT),
        (
            ".dark{color:red} p{background-image:linear-gradient(currentColor,white)}",
            TEXT,
        ),
        (".dark{color:red}", "<p>text <input value=x></p>"),
        (
            ".dark{color:red}",
            "<p>text <svg width=10 height=10><rect fill=currentColor width=5 height=5/></svg></p>",
        ),
        (".dark{color:red} p::before{content:'x'}", TEXT),
        (
            ".dark{color:red} p{text-overflow:ellipsis;overflow:hidden;white-space:nowrap;width:20px}",
            TEXT,
        ),
        (".dark p{color:red;font-weight:bold}", TEXT),
    ] {
        let mut page = page(css, body);
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        assert!(!recolor(&mut page, &mut retained, "dark"), "{css}");
    }
}
