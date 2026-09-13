use super::*;

#[test]
fn snapping_a_computed_border_again_does_not_lose_pixels_at_fractional_dpi() {
    for dpi in 96..=240 {
        for width in [0.1, 1.0, 2.0, 7.0, 13.0, 24.0, 120.0] {
            let mut style = ComputedStyle::initial();
            style.border_width = uniform_edges(Length::Px(width));
            let scale = dpi as f32 / 96.0;
            style.snap_border_widths(scale);
            let once = style.border_width;
            style.snap_border_widths(scale);
            assert_eq!(style.border_width, once, "DPI {dpi}, width {width}");
        }
    }
}

#[test]
fn border_width_snapping_is_shared_by_cssom_pseudos_and_dpi_refresh() {
    let dom = dom::parse(
        "<style>#target{border:1px solid;border-right-width:6px;border-bottom-width:.1px;border-left-width:0}#target::before{content:'x';border:1px solid}</style><div id=target></div>",
    );
    let target = dom.elements_named("div").next().unwrap();
    let environment = |scale| media::MediaEnvironment::new(800.0, 600.0, scale, false);
    let mut styles = StyleSet::from_sources_for_media_environment(&dom, "", &[], environment(1.25));
    let expected = Edges {
        top: Length::Px(0.8),
        right: Length::Px(5.6),
        bottom: Length::Px(0.8),
        left: Length::Px(0.0),
    };
    assert_eq!(styles.get(&target).border_width, expected);
    assert_eq!(
        resolved_property_value(styles.get(&target), "border-right-width").as_deref(),
        Some("5.6px")
    );
    let pseudo = styles
        .computed_style_for_pseudo(&target, PseudoElement::Before)
        .unwrap();
    assert_eq!(pseudo.border_width.top, Length::Px(0.8));
    let changes = styles.rebuild_rules_for_media_environment(&dom, "", &[], environment(2.0), &[]);
    assert!(changes.layout_changed);
    assert_eq!(styles.get(&target).border_width.top, Length::Px(1.0));
    assert_eq!(styles.get(&target).border_width.right, Length::Px(6.0));
    assert_eq!(styles.get(&target).border_width.bottom, Length::Px(0.5));
}

#[test]
fn border_width_snapping_keeps_integral_pixels_and_resolves_font_lengths_first() {
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let dom = dom::parse(
            "<div style='font-size:20px;border:.3em solid'><div style='border-width:inherit'></div></div>",
        );
        let styles = StyleSet::from_sources_for_media_environment(
            &dom,
            "",
            &[],
            media::MediaEnvironment::new(800.0, 600.0, scale, false),
        );
        let expected = (6.0_f32 * scale).floor() / scale;
        for node in dom.elements_named("div") {
            assert!(
                (styles
                    .get(&node)
                    .border_width
                    .top
                    .resolve(0.0, 20.0)
                    .unwrap()
                    - expected)
                    .abs()
                    < 0.00001,
                "scale {scale}"
            );
        }
    }
}

fn style(css: &str) -> ComputedStyle {
    let dom = dom::parse(&format!("<div id=target style='{css}'></div>"));
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = Node::descendants(&dom.document)
        .find(|n| n.attr("id").as_deref() == Some("target"))
        .unwrap();
    styles.get(&node).clone()
}

#[test]
fn border_color_expansion_longhands_and_invalid_values_are_atomic() {
    let red = Color::rgb(255, 0, 0);
    let green = Color::rgb(0, 128, 0);
    let blue = Color::rgb(0, 0, 255);
    for (value, expected) in [
        ("red", [red; 4]),
        ("red green", [red, green, red, green]),
        ("red green blue", [red, green, blue, green]),
        ("red green blue black", [red, green, blue, Color::BLACK]),
        ("rgb(255, 0, 0) /* gap */ green", [red, green, red, green]),
    ] {
        assert_eq!(
            style(&format!("border-color:{value}")).resolved_border_colors(),
            expected
        );
    }
    assert_eq!(
        style("border:2px solid red;border-left:8px solid blue").resolved_border_colors(),
        [red, red, red, blue]
    );
    assert_eq!(
        style("border-color:red;border-top-color:blue").resolved_border_colors(),
        [blue, red, red, red]
    );
    for invalid in ["red nonsense", "red green blue black white", "", "red,blue"] {
        assert_eq!(
            style(&format!("border-color:green;border-color:{invalid}")).resolved_border_colors(),
            [green; 4]
        );
    }
    assert_eq!(
        style("border-color:red;border-top-color:blue green").resolved_border_colors(),
        [red; 4]
    );
}

#[test]
fn currentcolor_tracks_final_color_and_shorthands_reset_only_their_edge() {
    let blue = Color::rgb(0, 0, 255);
    let red = Color::rgb(255, 0, 0);
    assert_eq!(
        style("border:1px solid;color:blue").resolved_border_colors(),
        [blue; 4]
    );
    assert_eq!(
        style("border-color:red;border-left:1px solid;color:blue").resolved_border_colors(),
        [red, red, red, blue]
    );
    assert_eq!(
        style("border-color:red!important;border-left-color:blue").resolved_border_colors(),
        [red; 4]
    );
    assert_eq!(
        style("border:1px solid red;border:2px solid blue nonsense").resolved_border_colors(),
        [red; 4]
    );
    assert_eq!(
        style("border:1px solid red;border-color:initial;color:blue").resolved_border_colors(),
        [blue; 4]
    );
}

#[test]
fn css_wide_inheritance_preserves_independent_colors_and_currentcolor_keyword() {
    let dom = dom::parse(
        "<div style='color:red;border-color:currentColor green blue black'><div id=child style='color:purple;border-color:inherit;border-left-color:initial'></div></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let child = Node::descendants(&dom.document)
        .find(|n| n.attr("id").as_deref() == Some("child"))
        .unwrap();
    assert_eq!(
        styles.get(&child).resolved_border_colors(),
        [
            Color::rgb(128, 0, 128),
            Color::rgb(0, 128, 0),
            Color::rgb(0, 0, 255),
            Color::rgb(128, 0, 128)
        ]
    );
}

#[test]
fn serialization_queries_and_paint_invalidation_use_all_four_edges() {
    let before = style("border:2px solid red");
    let after = style("border:2px solid red;border-right-color:transparent");
    assert_ne!(before, after);
    assert!(before.layout_equivalent(&after));
    assert_eq!(
        resolved_property_value(&after, "border-right-color").as_deref(),
        Some("rgba(0, 0, 0, 0)")
    );
    assert_eq!(
        resolved_property_value(&before, "border-color").as_deref(),
        Some("rgb(255, 0, 0)")
    );
    for (query, expected) in [
        ("(border-color:red blue)", true),
        ("(border-left-color:currentColor)", true),
        ("(border-left-color:red blue)", false),
        ("(border-color:red bad)", false),
    ] {
        assert_eq!(supports::supports_matches(query), expected, "{query}");
    }
    assert_eq!(
        after.painted_border_colors(Color::WHITE)[1],
        Color::TRANSPARENT
    );
}
