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
