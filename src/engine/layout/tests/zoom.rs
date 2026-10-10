use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn bounds(html: &str, ids: &[&str]) -> Vec<RectF> {
    let page = Page::parse(html, "https://example.test/");
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    ids.iter()
        .map(|id| {
            let node = Node::descendants(&page.dom.document)
                .find(|n| n.attr("id").as_deref() == Some(id))
                .unwrap();
            layout.node_bounds[&node.id()]
        })
        .collect()
}

#[test]
fn nested_zoom_scales_lengths_without_double_scaling_inherited_fonts() {
    let rects = bounds(
        "<style>body{margin:0}#a{zoom:2;width:100px;height:100px;font-size:10px}#b{zoom:.5;width:40px;height:2em}#c{width:50%;height:10px}</style><div id=a><div id=b></div><div id=c></div></div>",
        &["a", "b", "c"],
    );
    assert_eq!((rects[0].width, rects[0].height), (200.0, 200.0));
    assert_eq!((rects[1].width, rects[1].height), (40.0, 20.0));
    assert_eq!((rects[2].width, rects[2].height), (100.0, 20.0));
}

#[test]
fn zoom_changes_grid_contributions_and_fixed_tracks() {
    let rects = bounds(
        "<style>body{margin:0}#grid{display:grid;grid-template-columns:auto 1fr auto;width:800px}#left{zoom:.5;width:420px;height:40px}#right{zoom:.5;width:290px;height:40px}#inside{display:grid;grid-template-columns:40px 40px;gap:4px}#cell{height:20px}</style><div id=grid><div id=left><div id=inside><div id=cell></div><div></div></div></div><div></div><div id=right></div></div>",
        &["left", "right", "cell"],
    );
    assert_eq!(rects[0].width, 210.0);
    assert_eq!(rects[1].width, 145.0);
    assert_eq!((rects[2].width, rects[2].height), (20.0, 10.0));
}

#[test]
fn zoom_scales_replaced_attribute_sizes_and_retains_paint_hit_boxes() {
    let page = Page::parse(
        "<style>body{margin:0}div{zoom:.5}</style><div><a href='/next'><img src='/image.png' width=100 height=40></a></div>",
        "https://example.test/",
    );
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let img = page.dom.elements_named("img").next().unwrap();
    let rect = layout.node_bounds[&img.id()];
    assert_eq!((rect.width, rect.height), (50.0, 20.0));
}

#[test]
fn zero_zoom_uses_one_and_mutation_invalidates_layout() {
    let page = Page::parse(
        "<style>body{margin:0}</style><div id=a style='zoom:0;width:100px;height:20px'></div>",
        "https://example.test/",
    );
    let a = page.dom.elements_named("div").next().unwrap();
    let before = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    a.set_attr("style", "zoom:2;width:100px;height:20px");
    let after = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(before.node_bounds[&a.id()].width, 100.0);
    assert_eq!(after.node_bounds[&a.id()].width, 200.0);
}
