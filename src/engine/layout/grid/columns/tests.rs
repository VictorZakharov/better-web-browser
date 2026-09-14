use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn bounds(markup: &str) -> (RectF, RectF) {
    let page = Page::parse(markup, "https://example.test/");
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let geometry =
        layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
    assert_eq!(output.node_bounds, geometry.node_bounds);
    let rect = |tag| output.node_bounds[&page.dom.elements_named(tag).next().unwrap().id()];
    (rect("article"), rect("aside"))
}

#[test]
fn intrinsic_sidebar_reserves_its_width_before_fr_consumes_remaining_space() {
    let (article, aside) = bounds(
        "<style>body{margin:0}main{display:grid;width:600px;grid-template-columns:minmax(0,1fr) min-content;column-gap:20px}aside{width:196px}</style><main><article>Content</article><aside>Appearance</aside></main>",
    );
    assert_eq!(article.width, 384.0);
    assert_eq!(aside.x, 404.0);
    assert_eq!(aside.right(), 600.0);
}

#[test]
fn minmax_fixed_maximum_shrinks_to_leave_room_for_intrinsic_columns() {
    let (article, aside) = bounds(
        "<style>body{margin:0}main{display:grid;width:500px;grid-template-columns:minmax(0,400px) min-content}aside{width:200px}</style><main><article>Content</article><aside>Appearance</aside></main>",
    );
    assert_eq!(article.width, 300.0);
    assert_eq!(aside.x, 300.0);
    assert_eq!(aside.right(), 500.0);
}

#[test]
fn min_content_max_content_and_auto_are_distinct_sizing_modes() {
    for (keyword, expected) in [
        ("min-content", 32.0),
        ("max-content", 56.0),
        ("auto", 300.0),
    ] {
        let (article, _) = bounds(&format!(
            "<style>body{{margin:0}}main{{display:grid;width:400px;grid-template-columns:{keyword} 100px}}</style><main><article>AAAA BB</article><aside>Rail</aside></main>"
        ));
        assert_eq!(article.width, expected, "{keyword}");
    }
}

#[test]
fn spanned_header_does_not_assign_its_width_to_the_intrinsic_sidebar() {
    let (article, aside) = bounds(
        "<style>body{margin:0}main{display:grid;width:400px;grid-template-columns:minmax(0,1fr) min-content;grid-template-areas:'head head' 'body rail'}header{grid-area:head}article{grid-area:body}aside{grid-area:rail;width:100px}</style><main><header>A heading spanning both columns should wrap</header><article>Body</article><aside>Rail</aside></main>",
    );
    assert_eq!(article.width, 300.0);
    assert_eq!(aside.width, 100.0);
    assert_eq!(aside.right(), 400.0);
}

#[test]
fn fixed_minimum_freezes_before_redistributing_the_flex_fraction() {
    let (article, aside) = bounds(
        "<style>body{margin:0}main{display:grid;width:400px;grid-template-columns:minmax(300px,1fr) minmax(0,1fr)}</style><main><article>Body</article><aside>Rail</aside></main>",
    );
    assert_eq!(article.width, 300.0);
    assert_eq!(aside.width, 100.0);
}
