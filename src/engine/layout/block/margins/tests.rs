use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn box_for(page: &Page, output: &LayoutOutput, id: &str) -> RectF {
    let node = Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds[&node.id()]
}

#[test]
fn parent_child_margins_collapse_without_inflating_the_parent_height() {
    let page = Page::parse(
        "<style>body{margin:0}main{padding:1px}#parent{margin:20px 0 25px}\
         #child{height:10px;margin:30px 0 40px}#next{height:10px;margin-top:15px}</style>\
         <main><div id=parent><div id=child></div></div><div id=next></div></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(box_for(&page, &output, "parent").y, 31.0);
    assert_eq!(box_for(&page, &output, "parent").height, 10.0);
    assert_eq!(box_for(&page, &output, "child").y, 31.0);
    assert_eq!(box_for(&page, &output, "next").y, 81.0);
}

#[test]
fn nested_empty_margins_join_positive_and_negative_groups() {
    let page = Page::parse(
        "<style>body{margin:0}main{padding:1px}#a{height:10px;margin-bottom:20px}\
         #empty{margin:30px 0 -10px}#inner{margin:50px 0 -20px}\
         #next{height:10px;margin-top:15px}</style>\
         <main><div id=a></div><div id=empty><div id=inner></div></div><div id=next></div></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(box_for(&page, &output, "empty").height, 0.0);
    assert_eq!(
        box_for(&page, &output, "inner").y,
        box_for(&page, &output, "empty").y
    );
    assert_eq!(box_for(&page, &output, "next").y, 41.0);
}

#[test]
fn border_padding_formatting_context_and_percentage_height_bound_collapse() {
    for (rule, child_y, parent_height) in [
        ("border:1px solid", 51.0, 82.0),
        ("padding:1px", 51.0, 82.0),
        ("overflow:auto", 50.0, 80.0),
        ("height:50%", 30.0, 100.0),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{height:200px;overflow:hidden}}\
             #parent{{margin-top:20px;{rule}}}#child{{height:10px;margin:30px 0 40px}}</style>\
             <main><div id=parent><div id=child></div></div></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(box_for(&page, &output, "child").y, child_y, "{rule}");
        assert_eq!(
            box_for(&page, &output, "parent").height,
            parent_height,
            "{rule}"
        );
    }
}

#[test]
fn percentage_margins_use_each_owning_containing_block_width() {
    let page = Page::parse(
        "<style>body{margin:0}main{padding:1px;width:400px}#parent{width:200px}\
         #child{height:10px;margin:10% 0}</style><main><div id=parent><div id=child></div></div></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(box_for(&page, &output, "parent").y, 21.0);
    assert_eq!(box_for(&page, &output, "child").y, 21.0);
    assert_eq!(box_for(&page, &output, "parent").height, 10.0);
}

#[test]
fn sibling_margins_keep_positive_and_negative_extrema() {
    for (before, after, expected) in [(20, 30, 30.0), (20, -10, 10.0), (-20, -10, -20.0)] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}} main{{padding:1px}} div{{height:10px}}\
             #a{{margin-bottom:{before}px}} #b{{margin-top:{after}px}}</style>\
             <main><div id=a></div><div id=b></div></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let nodes = page.dom.elements_named("div").collect::<Vec<_>>();
        assert_eq!(
            output.node_bounds[&nodes[1].id()].y - output.node_bounds[&nodes[0].id()].bottom(),
            expected
        );
    }
}

#[test]
fn empty_sibling_chain_combines_the_entire_margin_set() {
    let page = Page::parse(
        "<style>body{margin:0} main{padding:1px} #a{height:10px;margin-bottom:20px}\
         #empty{margin-top:30px;margin-bottom:-10px} #b{height:10px;margin-top:15px}</style>\
         <main><div id=a></div><div id=empty></div><div id=b></div></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let nodes = page.dom.elements_named("div").collect::<Vec<_>>();
    assert_eq!(
        output.node_bounds[&nodes[2].id()].y - output.node_bounds[&nodes[0].id()].bottom(),
        20.0
    );
}

#[test]
fn empty_block_self_collapse_does_not_apply_to_formatting_contexts() {
    for (extra, expected) in [
        ("", 24.0),
        ("overflow:hidden", 48.0),
        ("padding-top:1px", 49.0),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}} main{{padding:1px}} #empty{{margin:24px 0;{extra}}}\
             #next{{height:10px}}</style><main><div id=empty></div><div id=next></div></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let next = page.dom.elements_named("div").last().unwrap();
        assert_eq!(output.node_bounds[&next.id()].y, 1.0 + expected, "{extra}");
    }
}

#[test]
fn grid_item_contains_a_collapsed_empty_child_margin_set() {
    let page = Page::parse(
        "<style>body{margin:0}main{display:grid;grid-template-columns:1fr}#empty{position:relative;margin:24px 0}#next{height:40px}</style><main><section>\n<div id=empty><!-- No rendered contents --></div>\n</section><section id=next></section></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let sections = page.dom.elements_named("section").collect::<Vec<_>>();
    assert_eq!(output.node_bounds[&sections[0].id()].height, 24.0);
    assert_eq!(output.node_bounds[&sections[1].id()].y, 24.0);
}
