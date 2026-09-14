use super::*;

#[test]
fn section_scroll_reveals_nested_scrollports_before_the_viewport() {
    let dom = crate::engine::dom::parse("<div><p id=section>Section</p></div>");
    let parent = dom.elements_named("div").next().unwrap();
    let target = dom.elements_named("p").next().unwrap();
    let boxes = HashMap::from([(
        parent.id(),
        ScrollBox {
            port: RectF {
                x: 20.0,
                y: 100.0,
                width: 200.0,
                height: 100.0,
            },
            content_width: 200.0,
            content_height: 600.0,
            scroll_y: true,
            ..ScrollBox::default()
        },
    )]);
    let geometry = HashMap::from([(
        target.id(),
        RectF {
            x: 20.0,
            y: 400.0,
            width: 100.0,
            height: 30.0,
        },
    )]);
    let scroll = scroll_to_fragment(
        &dom.document,
        "https://example.test/#section",
        &geometry,
        &boxes,
        1000.0,
    );
    assert_eq!(parent.scroll_offset.get(), (0.0, 300.0));
    assert_eq!(scroll, Some(100.0));
    assert_eq!(
        scroll_to_fragment(
            &dom.document,
            "https://example.test/#missing",
            &geometry,
            &boxes,
            1000.0
        ),
        None
    );
    assert_eq!(parent.scroll_offset.get(), (0.0, 300.0));
}

#[test]
fn fragments_select_ids_before_names_and_decode_without_form_semantics() {
    let dom = crate::engine::dom::parse(
        "<a name=section></a><p id=section></p><p id='café+tea'></p><p id=top></p><p id='%61'></p><p id=a></p>",
    );
    for (fragment, id) in [
        ("section", "section"),
        ("caf%C3%A9+tea", "café+tea"),
        ("top", "top"),
        ("%61", "%61"),
    ] {
        let Some(FragmentTarget::Element(node)) =
            select(&dom.document, &format!("https://example.test/#{fragment}"))
        else {
            panic!("{fragment}")
        };
        assert_eq!(node.attr("id").as_deref(), Some(id));
    }
    for fragment in ["", "TOP"] {
        assert!(matches!(
            select(&dom.document, &format!("https://example.test/#{fragment}")),
            Some(FragmentTarget::Top)
        ));
    }
    assert!(select(&dom.document, "https://example.test/#missing").is_none());
    assert!(is_same_document(
        "https://example.test/a?q=1#old",
        "https://example.test/a?q=1#new"
    ));
    assert!(!is_same_document(
        "https://example.test/a?q=1",
        "https://example.test/a?q=2#new"
    ));
    assert!(!is_same_document(
        "https://example.test/a#old",
        "https://example.test/a"
    ));
}
