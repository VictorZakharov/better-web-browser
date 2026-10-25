use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn bounds(page: &Page, output: &LayoutOutput, tag: &str, id: &str) -> RectF {
    let node = page
        .dom
        .elements_named(tag)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds[&node.id()]
}

#[test]
fn length_vertical_align_raises_atomic_inlines_and_the_line_box_reaches_the_strut() {
    // CSS 2.1 §10.8: a `vertical-align` length raises the box above the baseline, and the
    // line box spans from the highest box top down to the zero-height strut's baseline.
    let page = Page::parse(
        "<style>body{margin:0}div{font:0/0 serif}p{display:inline-block;margin:0;\
         vertical-align:2em;width:10px}#a{font-size:10px;height:20px}\
         #b{font-size:20px;height:40px}</style>\
         <div id=d><p id=a></p><p id=b></p></div>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    // #b: 40px tall raised 40px, so the line is 80px tall with the baseline at 80px.
    assert_eq!(bounds(&page, &output, "div", "d").height, 80.0);
    assert_eq!(bounds(&page, &output, "p", "b").y, 0.0);
    // #a: bottom 20px above the baseline, so its 20px box starts at 40px.
    assert_eq!(bounds(&page, &output, "p", "a").y, 40.0);
}

#[test]
fn shifted_lines_keep_inline_block_text_baselines_and_top_alignment() {
    // CSS 2.1 §10.8.1: an inline-block with text sits on its last line's baseline, not on
    // its bottom edge, and `top` aligns to the line box top, which grows to contain it.
    let page = Page::parse(
        "<style>body{margin:0}div{font:20px/20px serif}span{display:inline-block}\
         #s{vertical-align:10px;width:10px;height:10px}#t{vertical-align:top;height:50px}\
         </style><div id=d><span id=x>text</span><span id=s></span><span id=t></span></div>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    // Strut ascent 16; #s rises to 20 above the baseline; #x's baseline is 16 below its top.
    assert_eq!(bounds(&page, &output, "span", "s").y, 0.0);
    assert_eq!(bounds(&page, &output, "span", "x").y, 4.0);
    assert_eq!(bounds(&page, &output, "span", "t").y, 0.0);
    assert_eq!(bounds(&page, &output, "div", "d").height, 50.0);
}

#[test]
fn lines_holding_only_empty_undecorated_inline_boxes_are_zero_height() {
    // CSS 2.1 §9.4.2: a line box with no text, no in-flow content and no inline box with
    // margins, padding or borders is treated as zero-height (Acid3's `<map><area>` line).
    let page = Page::parse(
        "<style>body{margin:0;font:20px serif}p{margin:0}i{padding-left:1px}</style>\
         <p id=a>a</p>\n <script></script><span><b></b><form id=f></form></span>\
         <p id=e> <b><s></s></b> </p><p id=d><i></i></p>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let a = bounds(&page, &output, "p", "a");
    assert_eq!(bounds(&page, &output, "form", "f").y, a.bottom());
    assert_eq!(bounds(&page, &output, "p", "e").height, 0.0);
    // Padding on the inline box keeps its line.
    assert!(bounds(&page, &output, "p", "d").height > 0.0);
}

#[test]
fn a_line_ended_by_a_forced_break_keeps_its_height_even_with_only_empty_inline_boxes() {
    // CSS 2.1 §9.4.2 exempts nothing for `<br>`, but browsers keep the line, and editors
    // rely on it for empty paragraphs such as `<p><span></span><br></p>`.
    let page = Page::parse(
        "<style>body{margin:0;font:20px/20px serif}p{margin:0}</style>\
         <p id=s><span></span><br></p><p id=a><a name=x></a><br></p>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(bounds(&page, &output, "p", "s").height, 20.0);
    assert_eq!(bounds(&page, &output, "p", "a").height, 20.0);
}

#[test]
fn nbsp_only_inline_blocks_sit_on_their_text_baseline() {
    // CSS 2.1 §10.8.1: only an inline-block without in-flow line boxes uses its bottom
    // margin edge; a no-break space is content, not collapsible white space.
    let page = Page::parse(
        "<style>body{margin:0}div{font:20px/20px serif}span{display:inline-block}\
         #s{vertical-align:10px;width:10px;height:10px}</style>\
         <div><span id=x>&nbsp;</span><span id=s></span></div>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(bounds(&page, &output, "span", "x").y, 4.0);
}
