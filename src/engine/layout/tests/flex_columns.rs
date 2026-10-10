use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn rects(css: &str, markup: &str, ids: &[&str]) -> Vec<RectF> {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}{css}</style>{markup}"),
        "https://example.test/",
    );
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    ids.iter()
        .map(|id| {
            let node = Node::descendants(&page.dom.document)
                .find(|node| node.attr("id").as_deref() == Some(id))
                .unwrap();
            layout.node_bounds[&node.id()]
        })
        .collect()
}

#[test]
fn column_auto_margins_absorb_free_space_before_justification() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;width:200px;height:300px;align-items:center;justify-content:center}div{width:50px;height:20px;flex-shrink:0}#a{margin-top:10px;margin-bottom:auto}",
        "<main><div id=a></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!((bounds[0].x, bounds[0].y), (75.0, 10.0));
    assert_eq!((bounds[1].x, bounds[1].y), (75.0, 280.0));
}

#[test]
fn column_flex_basis_is_a_height_and_growth_respects_maximum() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;width:200px;height:400px;gap:10px}div{flex:1 1 100px;min-height:0}#a{max-height:120px}",
        "<main><div id=a></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!((bounds[0].width, bounds[1].width), (200.0, 200.0));
    assert_eq!((bounds[0].height, bounds[1].height), (120.0, 270.0));
    assert_eq!(bounds[1].y, 130.0);
}

#[test]
fn column_shrink_uses_content_sizes_and_freezes_explicit_minimum() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;width:200px;height:120px}div{height:100px;min-height:0}#a{min-height:90px}",
        "<main><div id=a></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!((bounds[0].height, bounds[1].height), (90.0, 30.0));
    assert_eq!(bounds[1].y, 90.0);
}

#[test]
fn column_reverse_keeps_physical_auto_margin_sides() {
    let bounds = rects(
        "main{display:flex;flex-direction:column-reverse;width:200px;height:300px}div{height:20px;flex-shrink:0}#a{margin-top:auto}",
        "<main><div id=a></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!((bounds[0].y, bounds[1].y), (280.0, 0.0));
}

#[test]
fn per_item_alignment_and_cross_axis_auto_margins_override_container() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;width:200px;align-items:stretch}div{height:20px}#a{width:40px;align-self:end}#b{width:40px;margin-left:auto;margin-right:auto;align-self:start}",
        "<main><div id=a></div><div id=b></div><div id=c></div></main>",
        &["a", "b", "c"],
    );
    assert_eq!(
        (bounds[0].x, bounds[1].x, bounds[2].width),
        (160.0, 80.0, 200.0)
    );
}

#[test]
fn indefinite_column_height_keeps_content_size_and_zero_auto_margins() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;width:200px;gap:10px}div{flex:1 1 auto;height:20px;margin-top:auto}",
        "<main id=m><div id=a></div><div id=b></div></main>",
        &["m", "a", "b"],
    );
    assert_eq!(bounds[0].height, 50.0);
    assert_eq!((bounds[1].height, bounds[2].y), (20.0, 30.0));
}

#[test]
fn zoomed_column_distributes_in_physical_coordinates() {
    let bounds = rects(
        "main{display:flex;flex-direction:column;zoom:.5;width:400px;height:600px}div{height:40px;flex-shrink:0}#b{margin-top:auto}",
        "<main><div id=a></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!(
        (bounds[1].width, bounds[1].y, bounds[1].height),
        (200.0, 280.0, 20.0)
    );
}
