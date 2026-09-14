use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn check(markup: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}}figure{{margin:0;display:table}}figcaption{{display:table-caption;caption-side:bottom}}.row{{display:table-row}}.cell{{display:table-cell}}.group{{display:table-row-group}}</style>{markup}"
        ),
        "https://example.test/",
    );
    let before = Node::descendants(&page.dom.document)
        .map(|n| (n.id(), n.parent().map(|p| p.id())))
        .collect::<Vec<_>>();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let geometry =
        layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
    assert_eq!(output.node_bounds, geometry.node_bounds);
    assert_eq!(output.resize_boxes, geometry.resize_boxes);
    assert_eq!(
        before,
        Node::descendants(&page.dom.document)
            .map(|n| (n.id(), n.parent().map(|p| p.id())))
            .collect::<Vec<_>>(),
        "anonymous boxes must not mutate the DOM"
    );
    (page, output)
}

fn rect(page: &Page, output: &LayoutOutput, id: &str) -> RectF {
    let node = Node::descendants(&page.dom.document)
        .find(|n| n.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds[&node.id()]
}

#[test]
fn css_table_wraps_ordinary_content_and_places_a_css_caption() {
    let (page, output) = check(
        "<figure id=figure><a><img id=image width=350 height=205></a><figcaption id=caption>Illustration caption with ordinary wrapping text</figcaption></figure>",
    );
    let figure = rect(&page, &output, "figure");
    let image = rect(&page, &output, "image");
    let caption = rect(&page, &output, "caption");
    assert_eq!(image.width, 350.0);
    assert!(caption.y >= image.bottom(), "{caption:?} / {image:?}");
    assert_eq!(figure.width, 350.0);
    assert!(figure.bottom() >= caption.bottom());
}

#[test]
fn responsive_image_max_width_does_not_collapse_an_automatic_table() {
    for maximum in ["100%", "calc(100% - 8px)"] {
        let (page, output) = check(&format!(
            "<figure id=figure><a><img id=image width=350 height=205 style='max-width:{maximum}'></a><figcaption id=caption>Caption</figcaption></figure>"
        ));
        assert_eq!(rect(&page, &output, "figure").width, 350.0);
        let expected = if maximum == "100%" { 350.0 } else { 342.0 };
        assert_eq!(rect(&page, &output, "image").width, expected);
    }
}

#[test]
fn author_auto_overrides_html_height_and_preserves_the_decoded_ratio() {
    for display in ["inline", "block"] {
        let mut page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{width:100px}}img{{display:{display};max-width:100%;height:auto}}</style><main><img src=/image.png width=350 height=205></main>"
            ),
            "https://example.test/",
        );
        page.images.insert(
            "https://example.test/image.png".into(),
            crate::engine::DecodedImage {
                width: 350,
                height: 205,
                bgra: vec![0; 350 * 205 * 4].into(),
            },
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let node = page.dom.elements_named("img").next().unwrap();
        let rect = output.node_bounds[&node.id()];
        assert_eq!(rect.width, 100.0);
        assert!(
            (rect.height - 100.0 * 205.0 / 350.0).abs() < 0.01,
            "{display}: {rect:?}"
        );
    }
}

#[test]
fn anonymous_rows_and_cells_preserve_consecutive_content_and_groups() {
    let (page, output) = check(
        "<main style='display:table;width:300px'><section class=group><div class=row><div class=cell id=a>A</div><span id=b>ordinary</span><span id=c> content</span><div class=cell id=d>D</div></div><div class=row><div class=cell id=e>E</div><div class=cell id=f>F</div><div class=cell id=g>G</div></div></section></main>",
    );
    let r = |id| rect(&page, &output, id);
    assert!(r("b").x >= r("a").right());
    assert!(r("c").right() <= r("d").x);
    assert_eq!(r("d").x, r("g").x);
    assert!(r("e").y >= r("a").bottom());
}

#[test]
fn misparented_css_cells_get_one_table_and_shared_rows() {
    let (page, output) = check(
        "<main><div class=row><div class=cell id=a>long label</div><div class=cell id=b>B</div></div> \n <div class=row><div class=cell id=c>C</div><div class=cell id=d>D</div></div></main>",
    );
    assert_eq!(rect(&page, &output, "b").x, rect(&page, &output, "d").x);
    assert!(rect(&page, &output, "c").y >= rect(&page, &output, "a").bottom());
}

#[test]
fn anonymous_content_translates_with_its_table_in_paint_and_geometry() {
    let (page, output) = check(
        "<figure style='transform:translate(40px,30px)'><a><img id=image width=100 height=80></a><figcaption id=caption>caption</figcaption></figure>",
    );
    assert_eq!(rect(&page, &output, "image").x, 40.0);
    assert_eq!(rect(&page, &output, "image").y, 30.0);
    assert!(rect(&page, &output, "caption").y >= 110.0);
}

#[test]
fn wide_floated_table_reserves_its_minimum_content_width() {
    let (page, output) = check(
        "<main style='width:400px'><table style='float:right;width:100px'><tr><td style='padding:0'><div style='width:250px;height:100px'></div></td></tr></table><p id=text style='margin:0'>A short paragraph beside the floating table</p></main>",
    );
    let table = page.dom.elements_named("table").next().unwrap();
    let table = output.node_bounds[&table.id()];
    assert_eq!(table.x, 150.0);
    for item in &output.items {
        if let DisplayItem::Text { rect, text, .. } = item
            && rect.y < table.bottom()
        {
            assert!(
                rect.right() <= table.x,
                "{text}: {rect:?} overlaps {table:?}"
            );
        }
    }
}
