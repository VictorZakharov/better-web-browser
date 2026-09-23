use super::*;

#[test]
fn relative_replaced_image_uses_containing_block_percentages_and_keeps_its_clip() {
    let mut page = Page::parse(
        "<style>body{margin:0}.tile{width:126px;height:126px;overflow:hidden}.tile img{position:relative;width:300%;height:300%;left:-100%;top:-100%}</style><div class=tile><img src=/sprite.png></div>",
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/sprite.png".into(),
        crate::engine::page::DecodedImage {
            width: 378,
            height: 378,
            bgra: vec![0; 378 * 378 * 4].into(),
        },
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let (index, rect) = output
        .items
        .iter()
        .enumerate()
        .find_map(|(index, item)| match item {
            DisplayItem::Image { rect, .. } => Some((index, *rect)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (-126.0, -126.0, 378.0, 378.0)
    );
    assert_eq!(output.node_bounds[&image.id()], rect);
    let clipped = output.items[..index].iter().any(|item| {
        matches!(item, DisplayItem::BeginClip { bounds }
            if bounds.x == 0.0 && bounds.y == 0.0 && bounds.width == 126.0 && bounds.height == 126.0)
    });
    assert!(
        clipped,
        "the original large image must be clipped, not resized to one tile"
    );
}
