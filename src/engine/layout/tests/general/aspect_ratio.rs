use super::*;

fn bounds_for(html: &str, tag: &str) -> RectF {
    let page = Page::parse(html, "https://example.com/");
    let node = page.dom.elements_named(tag).next().unwrap();
    layout_page(&page, 800.0, 600.0, &mut FixedMeasurer).node_bounds[&node.id()]
}

#[test]
fn block_auto_height_follows_preferred_ratio() {
    let rect = bounds_for(
        "<style>body{margin:0}div{width:200px;aspect-ratio:2/1}</style><div></div>",
        "div",
    );
    assert_eq!((rect.width, rect.height), (200.0, 100.0));
}

#[test]
fn explicit_height_wins_over_ratio_and_ratio_obeys_border_box_basis() {
    let explicit = bounds_for(
        "<style>body{margin:0}div{width:200px;height:40px;aspect-ratio:1/1}</style><div></div>",
        "div",
    );
    assert_eq!((explicit.width, explicit.height), (200.0, 40.0));
    let content_box = bounds_for(
        "<style>body{margin:0}div{width:180px;padding:10px;aspect-ratio:2/1}</style><div></div>",
        "div",
    );
    assert_eq!((content_box.width, content_box.height), (200.0, 110.0));
    let border_box = bounds_for(
        "<style>body{margin:0}div{width:200px;padding:10px;box-sizing:border-box;aspect-ratio:2/1}</style><div></div>",
        "div",
    );
    assert_eq!((border_box.width, border_box.height), (200.0, 100.0));
}

#[test]
fn block_with_auto_height_retains_content_minimum_unless_scrollable() {
    let visible = bounds_for(
        "<style>body{margin:0}div{width:100px;aspect-ratio:1/1}p{height:150px;margin:0}</style><div><p></p></div>",
        "div",
    );
    assert_eq!(visible.height, 150.0);
    let scrollable = bounds_for(
        "<style>body{margin:0}div{width:100px;aspect-ratio:1/1;overflow:auto}p{height:150px;margin:0}</style><div><p></p></div>",
        "div",
    );
    // The visible vertical scrollbar occupies 15px of the content width.
    assert_eq!(scrollable.height, 85.0);
}

fn image_bounds(css: &str, block: bool) -> RectF {
    let mut page = Page::parse(
        &format!("<style>body{{margin:0}}img{{{css}}}</style><img src=/ratio.png>"),
        "https://example.com/",
    );
    page.images.insert(
        "https://example.com/ratio.png".into(),
        crate::engine::page::DecodedImage {
            width: 200,
            height: 100,
            bgra: vec![0; 200 * 100 * 4].into(),
        },
    );
    let image = page.dom.elements_named("img").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let rect = output.node_bounds[&image.id()];
    if block {
        assert!(css.contains("display:block"));
    }
    rect
}

#[test]
fn replaced_images_transfer_the_selected_ratio_in_both_directions() {
    let intrinsic = image_bounds("width:100px;aspect-ratio:auto 1/1", false);
    assert_eq!((intrinsic.width, intrinsic.height), (100.0, 50.0));
    let overridden = image_bounds("width:100px;aspect-ratio:1/1", false);
    assert_eq!((overridden.width, overridden.height), (100.0, 100.0));
    let from_height = image_bounds("height:50px;aspect-ratio:1/1", false);
    assert_eq!((from_height.width, from_height.height), (50.0, 50.0));
    let block = image_bounds("display:block;width:100px;aspect-ratio:1/1", true);
    assert_eq!((block.width, block.height), (100.0, 100.0));
}
