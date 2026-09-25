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

#[test]
fn inline_object_fit_changes_pixels_but_not_cssom_element_bounds() {
    let mut page = Page::parse(
        "<style>body{margin:0}img{width:200px;height:100px;object-fit:contain;object-position:right top}</style><img src=/square.png>",
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/square.png".into(),
        crate::engine::page::DecodedImage {
            width: 100,
            height: 100,
            bgra: vec![0; 100 * 100 * 4].into(),
        },
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let box_rect = output.node_bounds[&image.id()];
    assert_eq!((box_rect.width, box_rect.height), (200.0, 100.0));
    let (painted, clip) = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image {
                rect, clip, url, ..
            } if url == "https://example.com/square.png" => Some((*rect, *clip)),
            _ => None,
        })
        .unwrap();
    assert_eq!(clip, Some(box_rect));
    assert_eq!(painted.x, box_rect.x + 100.0);
    assert_eq!(painted.y, box_rect.y);
    assert_eq!((painted.width, painted.height), (100.0, 100.0));
}

#[test]
fn block_object_fit_cover_crops_without_expanding_page_geometry() {
    let mut page = Page::parse(
        "<style>body{margin:0}img{display:block;width:200px;height:100px;object-fit:cover}</style><img src=/square.png>",
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/square.png".into(),
        crate::engine::page::DecodedImage {
            width: 100,
            height: 100,
            bgra: vec![0; 100 * 100 * 4].into(),
        },
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let box_rect = output.node_bounds[&image.id()];
    let (painted, clip) = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image {
                rect, clip, url, ..
            } if url == "https://example.com/square.png" => Some((*rect, *clip)),
            _ => None,
        })
        .unwrap();
    assert_eq!(clip, Some(box_rect));
    assert_eq!((box_rect.width, box_rect.height), (200.0, 100.0));
    assert_eq!(
        painted,
        RectF {
            x: box_rect.x,
            y: box_rect.y - 50.0,
            width: 200.0,
            height: 200.0,
        }
    );
    let mut fill_page = Page::parse(
        "<style>body{margin:0}img{display:block;width:200px;height:100px;object-fit:fill}</style><img src=/square.png>",
        "https://example.com/",
    );
    fill_page.images = page.images.clone();
    let fill_output = layout_page(&fill_page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(output.content_height, fill_output.content_height);
}
