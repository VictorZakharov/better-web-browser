use super::*;

fn node(value: u64) -> NodeId {
    NodeId::from_wire((1u128 << 64) | u128::from(value)).unwrap()
}

fn color_outcome(id: u64) -> ScriptOutcome {
    ScriptOutcome {
        render_requested: true,
        invalidation: RenderInvalidation {
            roots: vec![node(id)],
            impact: InvalidationImpact::STYLE,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn repeated_input_keeps_the_first_deadline_and_one_combined_paint() {
    let now = Instant::now();
    let mut paint = PendingColorPaint::new(now);
    assert_eq!(paint.timer_micros(), None);
    for (milliseconds, id) in [(0, 2), (5, 3), (15, 4)] {
        let mut outcome = color_outcome(id);
        paint.defer(
            &mut outcome,
            node(1),
            now + Duration::from_millis(milliseconds),
        );
        assert!(!outcome.render_requested);
        assert!(outcome.invalidation.is_empty());
        assert_eq!(paint.deadline, Some(now + COLOR_PAINT_INTERVAL));
    }
    let mut outcome = ScriptOutcome::default();
    paint.merge_due(&mut outcome, node(1), now + Duration::from_millis(15));
    assert!(!outcome.render_requested);
    paint.clock_advanced(now + Duration::from_millis(15));
    assert_eq!(paint.timer_micros(), Some(1_000));
    paint.merge_due(&mut outcome, node(1), now + COLOR_PAINT_INTERVAL);
    assert!(outcome.render_requested);
    assert_eq!(outcome.invalidation.roots, vec![node(2), node(3), node(4)]);
    assert_eq!(outcome.invalidation.impact, InvalidationImpact::PAINT);
    assert_eq!(paint.timer_micros(), None);
    let mut next = ScriptOutcome::default();
    paint.merge_due(&mut next, node(1), now + Duration::from_secs(1));
    assert!(!next.render_requested);
}

#[test]
fn deferred_visual_work_does_not_take_input_effects_or_wheel_distance() {
    let now = Instant::now();
    let mut paint = PendingColorPaint::new(now);
    let mut input = color_outcome(2);
    input.executed = 3;
    input.mutation_count = 4;
    input.console = vec!["observed input".into()];
    input.errors = vec!["author error".into()];
    input.navigation_url = Some("https://example.test/next".into());
    input.viewport_scroll_y = Some(200.0);
    input.viewport_wheel_delta_y = -12.5;
    paint.defer(&mut input, node(1), now);
    assert_eq!((input.executed, input.mutation_count), (3, 4));
    assert_eq!(input.console, ["observed input"]);
    assert_eq!(input.errors, ["author error"]);
    assert_eq!(
        input.navigation_url.as_deref(),
        Some("https://example.test/next")
    );
    assert_eq!(input.viewport_scroll_y, Some(200.0));
    assert_eq!(input.viewport_wheel_delta_y, -12.5);
    let mut repaint = ScriptOutcome::default();
    paint.merge_due(&mut repaint, node(1), now + COLOR_PAINT_INTERVAL);
    assert_eq!((repaint.executed, repaint.mutation_count), (0, 0));
    assert!(repaint.console.is_empty() && repaint.errors.is_empty());
    assert!(repaint.navigation_url.is_none() && repaint.viewport_scroll_y.is_none());
    assert_eq!(repaint.viewport_wheel_delta_y, 0.0);
}

#[test]
fn full_rebuild_absorbs_pending_colors_without_a_second_paint() {
    let now = Instant::now();
    let mut paint = PendingColorPaint::new(now);
    paint.defer(&mut color_outcome(2), node(1), now);
    paint.clear();
    assert_eq!(paint.timer_micros(), None);
    let mut outcome = ScriptOutcome::default();
    paint.merge_due(&mut outcome, node(1), now + Duration::from_secs(1));
    assert!(!outcome.render_requested);
    paint.defer(&mut color_outcome(3), node(1), now + Duration::from_secs(1));
    assert_eq!(
        paint.deadline,
        Some(now + Duration::from_secs(1) + COLOR_PAINT_INTERVAL)
    );
}

#[test]
fn due_color_paint_preserves_later_author_geometry_and_scroll_order() {
    let now = Instant::now();
    let mut paint = PendingColorPaint::new(now);
    paint.defer(&mut color_outcome(2), node(1), now);
    let mut author = ScriptOutcome {
        render_requested: true,
        invalidation: RenderInvalidation::full(node(1)),
        viewport_scroll_y: Some(700.0),
        viewport_wheel_delta_y: 8.5,
        ..Default::default()
    };
    paint.merge_due(&mut author, node(1), now + COLOR_PAINT_INTERVAL);
    assert_eq!(author.invalidation.roots, vec![node(1)]);
    assert!(author.invalidation.impact.affects_layout());
    assert!(author.invalidation.impact.affects_style());
    assert_eq!(author.viewport_scroll_y, Some(700.0));
    assert_eq!(author.viewport_wheel_delta_y, 8.5);
}

#[test]
fn admission_requires_color_only_styles_and_no_geometry_or_content_changes() {
    let input = color_outcome(2);
    let style = StyleRefreshStats {
        changed_styles: 1,
        ..Default::default()
    };
    assert!(color_only_change(&input.invalidation, &style));
    assert!(!color_only_change(&RenderInvalidation::default(), &style));
    assert!(!color_only_change(
        &RenderInvalidation::full(node(1)),
        &style
    ));
    for invalidation in [
        RenderInvalidation {
            rebuild_style_rules: true,
            ..input.invalidation.clone()
        },
        RenderInvalidation {
            removed_nodes: vec![node(3)],
            ..input.invalidation.clone()
        },
    ] {
        assert!(!color_only_change(&invalidation, &style));
    }
    for unsupported in [
        StyleRefreshStats {
            layout_changed: true,
            ..style
        },
        StyleRefreshStats {
            removed_styles: 1,
            ..style
        },
        StyleRefreshStats {
            non_deferable_paint_changes: true,
            ..style
        },
        StyleRefreshStats {
            full_rebuild: true,
            ..style
        },
        StyleRefreshStats::default(),
    ] {
        assert!(!color_only_change(&input.invalidation, &unsupported));
    }
}

#[test]
fn deferred_paint_reuses_already_refreshed_hover_colors_without_recomputing_styles() {
    use crate::engine::Page;
    use crate::engine::css::Color;
    use crate::engine::dom::Node;

    let now = Instant::now();
    let mut page = Page::parse_scripted(
        "<style>p{color:red}p:hover{color:blue;text-decoration:underline}</style><p>hover</p>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800.0, 600.0);
    let target = page.dom.elements_named("p").next().unwrap();
    let root = page.dom.document.id();
    Node::update_hover_path(&mut Vec::new(), Some(target.clone()));
    let mut outcome = ScriptOutcome {
        render_requested: true,
        invalidation: RenderInvalidation {
            roots: vec![root],
            impact: InvalidationImpact::STYLE,
            ..Default::default()
        },
        ..Default::default()
    };
    let style = page.refresh_layout_styles_after_invalidation_for_viewport(
        800.0,
        600.0,
        &outcome.invalidation,
    );
    assert!(color_only_change(&outcome.invalidation, &style));
    page.refresh_inline_svg_colors();
    let mut paint = PendingColorPaint::new(now);
    paint.defer(&mut outcome, root, now);
    paint.merge_due(&mut outcome, root, now + COLOR_PAINT_INTERVAL);
    let checkpoint =
        page.refresh_resources_after_invalidation_for_viewport(800.0, 600.0, &outcome.invalidation);
    assert_eq!(checkpoint.recomputed_styles, 0);
    let current = page
        .cached_style_for_viewport(800.0, 600.0)
        .unwrap()
        .styles
        .get(&target.id())
        .unwrap();
    assert_eq!(current.color, Color::rgb(0, 0, 255));
    assert!(current.text_decoration_underline);
}

#[test]
fn a_late_hover_deadline_is_relative_to_the_clock_not_each_report() {
    let now = Instant::now();
    let mut paint = PendingColorPaint::new(now);
    let idle = Duration::from_secs(20);
    paint.defer(&mut color_outcome(2), node(1), now + idle);
    assert_eq!(paint.timer_micros(), Some(20_016_000));
    let advertised = Duration::from_micros(paint.timer_micros().unwrap());
    assert_eq!(advertised.saturating_sub(idle), COLOR_PAINT_INTERVAL);
    paint.defer(
        &mut color_outcome(3),
        node(1),
        now + idle + Duration::from_millis(9),
    );
    assert_eq!(paint.timer_micros(), Some(20_016_000));
    assert_eq!(
        advertised.saturating_sub(idle + Duration::from_millis(9)),
        Duration::from_millis(7),
    );
    paint.clock_advanced(now + idle + Duration::from_millis(10));
    assert_eq!(paint.timer_micros(), Some(6_000));
    let mut early = ScriptOutcome::default();
    paint.merge_due(&mut early, node(1), now + idle + Duration::from_millis(15));
    assert!(!early.render_requested);
    paint.merge_due(&mut early, node(1), now + idle + COLOR_PAINT_INTERVAL);
    assert!(early.render_requested);
}

#[test]
fn image_font_and_variable_style_changes_keep_full_resource_admission() {
    use crate::engine::Page;
    use crate::engine::dom::Node;

    for declaration in [
        "background-image:url(hover.png)",
        "font-family:HoverFace",
        "--resource:url(hover.png);background-image:var(--resource)",
    ] {
        let mut page = Page::parse_scripted("<p>hover</p>", "https://example.test/");
        // Match the normal installed-sheet font discovery contract. Inline
        // document font-face discovery is independent of input paint admission.
        page.add_stylesheet_from(
            "https://example.test/hover.css",
            format!(
                "@font-face{{font-family:HoverFace;src:url(hover.woff)}}p:hover{{{declaration}}}"
            ),
        );
        page.refresh_resources_for_viewport(800.0, 600.0);
        assert!(
            !page.resources.iter().any(|resource| match resource {
                crate::engine::PageResource::Image { url } => url.ends_with("hover.png"),
                crate::engine::PageResource::Font { url, .. } => url.ends_with("hover.woff"),
                _ => false,
            }),
            "resource must first become used by hover: {declaration}"
        );
        let target = page.dom.elements_named("p").next().unwrap();
        Node::update_hover_path(&mut Vec::new(), Some(target));
        let invalidation = RenderInvalidation {
            roots: vec![page.dom.document.id()],
            impact: InvalidationImpact::STYLE,
            ..Default::default()
        };
        let style =
            page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, &invalidation);
        assert!(!color_only_change(&invalidation, &style), "{declaration}");
        page.refresh_resources_after_invalidation_for_viewport(
            800.0,
            600.0,
            &RenderInvalidation::default(),
        );
        assert!(
            page.resources.iter().any(|resource| match resource {
                crate::engine::PageResource::Image { url } => url.ends_with("hover.png"),
                crate::engine::PageResource::Font { url, .. } => url.ends_with("hover.woff"),
                _ => false,
            }),
            "{declaration}"
        );
    }
}
