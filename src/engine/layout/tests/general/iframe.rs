use super::*;

#[test]
fn iframe_has_replaced_intrinsic_size_without_an_image_resource() {
    for display in ["inline", "block"] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}iframe{{display:{display}}}</style><iframe srcdoc='child'></iframe>"
            ),
            "https://example.com/",
        );
        let frame = page.dom.elements_named("iframe").next().unwrap();
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let painted = output.items.iter().find_map(|item| match item {
            DisplayItem::EmbeddedFrame { rect, node_id } if *node_id == frame.id() => Some(*rect),
            _ => None,
        });
        let rect = painted.expect("the child browsing context has a paint box");
        assert_eq!((rect.width, rect.height), (300.0, 150.0), "{display}");
        assert!(!output.items.iter().any(|item| {
            matches!(item, DisplayItem::Image { url, .. } if url.contains("srcdoc"))
        }));
    }
}

#[test]
fn iframe_width_does_not_scale_its_default_height_like_an_image() {
    for display in ["inline", "block"] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}iframe{{display:{display};width:400px}}</style><iframe srcdoc='child'></iframe>"
            ),
            "https://example.com/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let rect = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::EmbeddedFrame { rect, .. } => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert_eq!((rect.width, rect.height), (400.0, 150.0), "{display}");
    }
}

#[test]
fn object_renders_either_its_content_navigable_or_its_fallback_children() {
    // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-object-element
    for (display, navigable) in [("inline", true), ("block", true), ("block", false)] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}object{{display:{display}}}</style><object data=doc.html>FAIL</object>"
            ),
            "https://example.com/",
        );
        let object = page.dom.elements_named("object").next().unwrap();
        object.set_object_represents_navigable(navigable);
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let frame = output.items.iter().find_map(|item| match item {
            DisplayItem::EmbeddedFrame { rect, node_id } if *node_id == object.id() => Some(*rect),
            _ => None,
        });
        let fallback = output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if &**text == "FAIL"));
        assert_eq!(
            frame.map(|rect| (rect.width, rect.height)),
            navigable.then_some((300.0, 150.0)),
            "{display}"
        );
        assert_eq!(fallback, !navigable, "{display}");
    }
}

#[test]
fn fixed_fallback_object_contains_a_navigable_object_that_hides_its_children() {
    // Acid3 test 16. The hidden fallback holds a fixed box, which must not be laid out
    // either, whether the navigable object is a fixed block or an inline box.
    for position in ["fixed", "static"] {
        let page = Page::parse(
            &format!(
                "<style>object{{position:{position};left:130.5px;top:84.3px}}</style>\
                 <object data=a><object data=b>\
                 <object style=position:fixed data=c>FAIL</object></object></object>"
            ),
            "https://example.com/",
        );
        let objects: Vec<_> = page.dom.elements_named("object").collect();
        objects[1].set_object_represents_navigable(true);
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert!(output.items.iter().any(|item| matches!(
            item,
            DisplayItem::EmbeddedFrame { node_id, .. } if *node_id == objects[1].id()
        )));
        let fallback = output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if &**text == "FAIL"));
        assert!(!fallback, "{position}");
    }
}
