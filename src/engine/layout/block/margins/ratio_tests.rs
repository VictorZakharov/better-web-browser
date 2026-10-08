//! Ratio-dependent block axes occupy flow even with no in-flow content.
use super::tests::box_for;
use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn layout(source: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(source, "https://example.test/");
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    (page, output)
}

#[test]
fn empty_ratio_box_keeps_its_height_between_collapsed_sibling_margins() {
    let (page, output) = layout(
        "<style>body{margin:0}#ratio{width:100px;aspect-ratio:2/1;margin:10px 0 20px}\
         #next{height:10px;margin-top:30px}</style><div id=ratio></div><div id=next></div>",
    );
    assert_eq!(box_for(&page, &output, "ratio").y, 10.0);
    assert_eq!(box_for(&page, &output, "ratio").height, 50.0);
    assert_eq!(box_for(&page, &output, "next").y, 90.0);
}

#[test]
fn ratio_parent_does_not_collapse_its_last_child_bottom_margin_outward() {
    let (page, output) = layout(
        "<style>body{margin:0}#ratio{width:100px;aspect-ratio:1/1}\
         #child{height:40px;margin-bottom:20px}#next{height:10px}</style>\
         <div id=ratio><div id=child></div></div><div id=next></div>",
    );
    assert_eq!(box_for(&page, &output, "ratio").height, 100.0);
    assert_eq!(box_for(&page, &output, "child").y, 0.0);
    assert_eq!(box_for(&page, &output, "next").y, 100.0);
}

#[test]
fn ratio_parent_still_allows_top_child_margin_collapse() {
    let (page, output) = layout(
        "<style>body{margin:0}#ratio{width:100px;aspect-ratio:1/1;margin-top:10px}\
         #child{height:40px;margin-top:20px}#next{height:10px}</style>\
         <div id=ratio><div id=child></div></div><div id=next></div>",
    );
    assert_eq!(box_for(&page, &output, "ratio").y, 20.0);
    assert_eq!(box_for(&page, &output, "child").y, 20.0);
    assert_eq!(box_for(&page, &output, "next").y, 120.0);
}

#[test]
fn explicit_zero_height_is_not_a_ratio_dependent_axis() {
    let (page, output) = layout(
        "<style>body{margin:0}#ratio{width:100px;height:0;aspect-ratio:1/1;\
         margin:10px 0 20px}#next{height:10px;margin-top:30px}</style>\
         <div id=ratio></div><div id=next></div>",
    );
    assert_eq!(box_for(&page, &output, "ratio").height, 0.0);
    assert_eq!(box_for(&page, &output, "next").y, 30.0);
}
