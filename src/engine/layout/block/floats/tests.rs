use super::*;
mod flow_root;
use crate::engine::layout::test_support::FixedMeasurer;

fn boxes(source: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!("<style>body{{margin:0;font:16px sans-serif}} p{{margin:0}}</style>{source}"),
        "https://example.test/",
    );
    let layout = layout_page(&page, 600.0, 800.0, &mut FixedMeasurer);
    (page, layout)
}
fn rect(page: &Page, layout: &LayoutOutput, id: &str) -> RectF {
    let node = Node::descendants(&page.dom.document)
        .find(|n| n.attr("id").as_deref() == Some(id))
        .unwrap();
    layout.node_bounds[&node.id()]
}

#[test]
fn cleared_right_floats_stack_without_consuming_the_article_column() {
    let (page, layout) = boxes(
        "<style>.aside{float:right;clear:right;width:200px;height:100px;margin:0 0 10px 20px}</style><div id=a class=aside></div><div id=b class=aside></div><div id=c class=aside></div><p id=text>Article remains beside the first box.</p>",
    );
    for (id, top) in [("a", 0.0), ("b", 110.0), ("c", 220.0)] {
        let r = rect(&page, &layout, id);
        assert_eq!((r.x, r.y, r.width), (400.0, top, 200.0), "{id}");
    }
    assert_eq!(rect(&page, &layout, "text").y, 0.0);
}

#[test]
fn non_fitting_floats_move_down_and_keep_their_specified_width() {
    let (page, layout) = boxes(
        "<div id=a style='float:left;width:400px;height:60px'></div><div id=b style='float:right;width:300px;height:20px'></div>",
    );
    assert_eq!(
        rect(&page, &layout, "b"),
        RectF {
            x: 300.0,
            y: 60.0,
            width: 300.0,
            height: 20.0
        }
    );
}

#[test]
fn clear_selects_only_the_requested_side_and_is_not_inherited() {
    let (page, layout) = boxes(
        "<div style='float:left;width:100px;height:40px'></div><div style='float:right;width:100px;height:100px'></div><div id=left style='clear:left;height:10px'></div><div id=both style='clear:both;height:10px'></div>",
    );
    assert_eq!(rect(&page, &layout, "left").y, 40.0);
    assert_eq!(rect(&page, &layout, "both").y, 100.0);
    let styles = page.style(600.0);
    let node = page.dom.elements_named("body").next().unwrap();
    assert_eq!(styles.get(&node).clear, Clear::None);
}

#[test]
fn clearance_crosses_ordinary_wrappers_but_not_independent_formatting_contexts() {
    let (page, layout) = boxes(
        "<div><div style='float:left;width:100px;height:60px'></div></div><div id=clear style='clear:left;height:10px'></div><div style='overflow:hidden'><div style='float:left;width:100px;height:40px'></div></div><div id=tail style='clear:both;height:10px'></div>",
    );
    assert_eq!(rect(&page, &layout, "clear").y, 60.0);
    assert_eq!(rect(&page, &layout, "tail").y, 110.0);
}

#[test]
fn line_boxes_recover_full_width_below_a_float() {
    let (page, layout) = boxes(
        "<style>p{line-height:20px;font-size:20px}</style><div style='float:left;width:300px;height:40px'></div><p id=text>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen</p>",
    );
    let lines = layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(lines.iter().any(|r| r.y < 40.0 && r.x >= 300.0));
    assert!(lines.iter().any(|r| r.y >= 40.0 && r.x == 0.0));
    assert_eq!(rect(&page, &layout, "text").width, 600.0);
}

#[test]
fn float_ignores_flex_basis_and_preserves_percentage_containing_block() {
    let (page, layout) = boxes(
        "<div style='float:left;width:200px;height:80px'></div><span id=box style='float:right;clear:right;width:50%;flex-basis:1px;height:30px'></span>",
    );
    assert_eq!(rect(&page, &layout, "box").width, 300.0);
    assert_eq!(rect(&page, &layout, "box").x, 300.0);
}

#[test]
fn auto_float_respects_fixed_width_descendants_instead_of_their_unwrapped_text() {
    let (page, layout) = boxes(
        "<div id=wrapper style='float:right'><span><table style='width:200px'><tr><td>A deliberately long wrapping sentence must not inflate its fixed width parent beyond the article.</td></tr></table></span></div><p>Article</p>",
    );
    assert_eq!(rect(&page, &layout, "wrapper").width, 200.0);
    let first = layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text { rect, text, .. } if text == "Article" => Some(rect),
            _ => None,
        })
        .unwrap();
    assert!(first.x == 0.0 && first.y < 20.0, "{first:?}");
}

#[test]
fn auto_floats_shrink_to_fit_but_do_not_break_unbreakable_content() {
    let text = "word ".repeat(60);
    let (page, layout) = boxes(&format!(
        "<div id=wrapping style='float:left'>{text}</div><div id=unbreakable style='float:right;clear:both'>{}</div>",
        "x".repeat(100)
    ));
    assert_eq!(rect(&page, &layout, "wrapping").width, 600.0);
    assert_eq!(rect(&page, &layout, "unbreakable").width, 800.0);
}

#[test]
fn auto_float_preserves_replaced_element_intrinsic_size() {
    let (page, layout) = boxes("<img id=image style='float:right' width=120 height=80>");
    assert_eq!(rect(&page, &layout, "image").width, 120.0);
}

#[test]
fn independent_blocks_keep_percentage_basis_and_min_max_constraints_beside_floats() {
    let (page, layout) = boxes(
        "<div style='float:left;width:200px;height:60px'></div><div id=percent style='overflow:hidden;width:50%;height:10px;flex-basis:1px'></div><div id=max style='overflow:hidden;max-width:200px;height:10px'></div><div id=min style='overflow:hidden;min-width:450px;height:10px'></div>",
    );
    assert_eq!(rect(&page, &layout, "percent").width, 300.0);
    assert_eq!(rect(&page, &layout, "percent").x, 200.0);
    assert_eq!(rect(&page, &layout, "max").width, 200.0);
    assert_eq!(rect(&page, &layout, "min").y, 60.0);
    assert_eq!(rect(&page, &layout, "min").width, 600.0);
}
