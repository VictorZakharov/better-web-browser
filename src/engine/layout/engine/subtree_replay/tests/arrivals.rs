//! Retained subtrees across image arrivals and layout-environment changes.
use super::*;

fn decode(page: &mut Page, id: &str, width: u32, height: u32) {
    let url = page.image_url(&element(page, id)).unwrap();
    page.images.insert(
        url,
        crate::engine::DecodedImage {
            width,
            height,
            bgra: vec![0; (width * height * 4) as usize].into(),
        },
    );
}

const IMAGES: &str = "<style>
    section{display:flow-root;margin:6px;padding:4px}
    .styled{width:5em;height:30px}
    .fit{object-fit:contain}
</style>
<section id=attributes><p>words before <img id=a src=a.png width=40 height=30> words after</p>
  <p>a second paragraph of words to measure</p></section>
<section id=css><img id=b class=styled src=b.png style=display:block><p>more words here</p></section>
<section id=auto><img id=c src=c.png width=40 style=display:block><p>auto height words</p></section>
<section id=contain><img id=d class='styled fit' src=d.png><p>contained image words</p></section>";

#[test]
fn images_whose_size_cannot_depend_on_them_do_not_invalidate_replays() {
    let mut page = Page::parse(IMAGES, "https://example.test/");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    // Both dimensions are definite and object-fit is fill: the box and the painted image are
    // the same before and after the image arrives, so these sections replay.
    decode(&mut page, "a", 400, 100);
    decode(&mut page, "b", 7, 90);
    let mut measured = Counting::default();
    let warm = layout(&page, &mut measured);
    assert_eq!(measured.calls, 0, "both sections replayed");
    assert_same_output(&warm, &assert_warm_matches_cold(&page));
    let bounds = |id: &str| warm.node_bounds[&element(&page, id).id()];
    assert_eq!((bounds("a").width, bounds("a").height), (40.0, 30.0));
    assert_eq!(bounds("b").height, 30.0);

    // A height that follows the natural ratio, or a fit that does, still depends on it.
    decode(&mut page, "c", 80, 20);
    decode(&mut page, "d", 10, 40);
    let warm = assert_warm_matches_cold(&page);
    assert_eq!(warm.node_bounds[&element(&page, "c").id()].height, 10.0);
}

#[test]
fn the_pass_after_an_environment_change_is_retained_for_the_next() {
    let page = super::page();
    settle(&page);
    // A scrollbar appearing narrows the viewport: nothing retained still applies. The pass at
    // the new width is retained right away instead of after another full pass.
    let narrow = WIDTH - 17.0;
    let mut first = Counting::default();
    layout_page_with_style_viewport(&page, narrow, HEIGHT, WIDTH, &mut first);
    let mut second = Counting::default();
    layout_page_with_style_viewport(&page, narrow, HEIGHT, WIDTH, &mut second);
    assert!(
        second.calls * 4 < first.calls,
        "the second pass at the new width replays ({} vs {})",
        second.calls,
        first.calls
    );
}
