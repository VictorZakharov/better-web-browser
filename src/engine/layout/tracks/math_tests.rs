use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn rectangles(style: &str, font: f32) -> Vec<RectF> {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}} main{{display:grid;width:300px;font-size:{font}px;{style}}}
            div{{height:10px;margin:0;padding:0;border:0;min-width:0}}</style>
            <main><div></div><div></div><div></div></main>"
        ),
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    page.dom
        .elements_named("div")
        .map(|node| output.node_bounds[&node.id()])
        .collect()
}

#[test]
fn calculated_repeat_count_places_actual_items_in_three_columns() {
    for expression in ["calc(1 + 2)", "round(2.5)", "sqrt(9)", "calc(12 / 4)"] {
        let boxes = rectangles(
            &format!("grid-template-columns:repeat({expression},50px);column-gap:10px"),
            16.0,
        );
        for (index, rect) in boxes.iter().enumerate() {
            assert_eq!(rect.x, index as f32 * 60.0, "{expression}");
            assert_eq!(rect.y, 0.0, "{expression}");
            assert_eq!(rect.width, 50.0, "{expression}");
        }
    }
}

#[test]
fn calculation_operators_inside_tracks_retain_outer_list_boundaries() {
    let boxes = rectangles(
        "grid-template-columns:calc(120px / 2) min(30%,80px) hypot(3px,4px)",
        16.0,
    );
    for (index, (x, width)) in [(0.0, 60.0), (60.0, 80.0), (140.0, 5.0)].iter().enumerate() {
        assert_eq!(boxes[index].x, *x);
        assert_eq!(boxes[index].width, *width);
    }
}

#[test]
fn repeat_groups_share_font_context_and_resolve_each_track() {
    for font in [10.0, 30.0] {
        let boxes = rectangles(
            "grid-template-columns:repeat(calc(2),max(1em,20px) 30px)",
            font,
        );
        let first = font.max(20.0);
        assert_eq!(boxes[0].width, first);
        assert_eq!(boxes[1].x, first);
        assert_eq!(boxes[1].width, 30.0);
        assert_eq!(boxes[2].x, first + 30.0);
        assert_eq!(boxes[2].width, first);
    }
}

#[test]
fn invalid_declaration_cannot_replace_a_valid_track_list() {
    for invalid in ["NaNfr", "minmax(1fr,2fr)", "repeat(0,50px)", "50px junk"] {
        let boxes = rectangles(
            &format!("grid-template-columns:50px 50px 50px;grid-template-columns:{invalid}"),
            16.0,
        );
        assert!(boxes.iter().all(|rect| rect.width == 50.0), "{invalid}");
        assert_eq!(boxes[2].x, 100.0, "{invalid}");
    }
}

#[test]
fn zero_and_fractional_factors_do_not_turn_into_default_one_fraction() {
    let boxes = rectangles("grid-template-columns:0fr .5fr 1fr", 16.0);
    assert_eq!(boxes[0].width, 0.0);
    assert_eq!(boxes[1].width, 100.0);
    assert_eq!(boxes[2].width, 200.0);
    assert_eq!(boxes[2].x, 100.0);
}

#[test]
fn gutters_do_not_reduce_the_percentage_track_basis() {
    for expression in ["20%", "calc(10% + 10%)", "max(10px,20%)"] {
        let boxes = rectangles(
            &format!("grid-template-columns:{expression} 1fr 1fr;column-gap:10px"),
            16.0,
        );
        assert_eq!(boxes[0].width, 60.0, "{expression}");
        assert_eq!(boxes[1].x, 70.0, "{expression}");
        assert_eq!(boxes[1].width, 110.0, "{expression}");
        assert_eq!(boxes[2].x, 190.0, "{expression}");
        assert_eq!(boxes[2].width, 110.0, "{expression}");
    }
}

#[test]
fn minmax_fixed_row_maximum_cannot_undercut_its_minimum() {
    for (minimum, maximum) in [("40px", "20px"), ("max(20px,40px)", "calc(10px + 10px)")] {
        let boxes = rectangles(
            &format!("grid-template-rows:minmax({minimum},{maximum}) 20px 20px"),
            16.0,
        );
        assert_eq!(boxes[1].y, 40.0);
        assert_eq!(boxes[2].y, 60.0);
    }
}
