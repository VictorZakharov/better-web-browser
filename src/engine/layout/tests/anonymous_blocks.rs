use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn clamped_block_descendants_of_inline_links_remain_boxes_and_links() {
    let page = Page::parse(
        "<style>body{margin:0}ul{display:flex;flex-wrap:wrap;margin:0;padding:0;\
         width:652px;list-style:none}li{display:block;width:270px}a{display:inline}\
         h3,p{display:block;margin:0}p{line-height:20px}.clamp{overflow:hidden;\
         display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2}</style>\
         <ul><li><a href='/result'><h3>Speedtest</h3><p><span id='clamp' class='clamp'>\
         A deliberately long result summary which wraps across several lines and must be \
         clamped while retaining the surrounding anchor's activation target.</span></p></a></li></ul>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let clamp = page
        .dom
        .elements_named("span")
        .find(|node| node.attr("id").as_deref() == Some("clamp"))
        .unwrap();
    let paragraph = page.dom.elements_named("p").next().unwrap();
    let mut text = output.items.iter().filter_map(|item| match item {
        DisplayItem::Text { text, link, .. } => Some((text, link)),
        _ => None,
    });

    assert_eq!(output.node_bounds[&clamp.id()].height, 40.0);
    assert!(output.node_bounds.contains_key(&paragraph.id()));
    assert!(text.clone().any(|(text, _)| text.ends_with('…')));
    assert!(text.all(|(_, link)| link.as_deref() == Some("https://example.test/result")));
}
