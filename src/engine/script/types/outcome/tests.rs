use super::*;

fn pixels() -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    outcome.merge_render_request(true, Some(RenderScope::SurfacePixels));
    outcome
}

#[test]
fn surface_outcomes_coalesce_without_upgrading_quiet_tasks() {
    let mut outcome = pixels();
    assert!(outcome.is_surface_repaint());
    outcome.merge_render_request(false, Some(RenderScope::Layout));
    outcome.merge_render_request(true, Some(RenderScope::SurfacePixels));
    assert!(outcome.is_surface_repaint());
    outcome.request_full_render();
    outcome.merge_render_request(true, Some(RenderScope::SurfacePixels));
    assert!(!outcome.is_surface_repaint());
    assert_eq!(outcome.render_scope, Some(RenderScope::Layout));
}

#[test]
fn unknown_request_scope_never_gets_downgraded_by_pixel_work() {
    let mut outcome = ScriptOutcome {
        render_requested: true,
        ..Default::default()
    };
    outcome.merge_render_request(true, Some(RenderScope::SurfacePixels));
    assert_eq!(outcome.render_scope, Some(RenderScope::Layout));
    assert!(!outcome.is_surface_repaint());
    let mut outcome = pixels();
    outcome.merge_render_request(true, None);
    assert!(!outcome.is_surface_repaint());
}

#[test]
fn invalidation_evidence_and_nonpixel_state_reject_the_shortcut() {
    let root = crate::engine::dom::parse("<p>text</p>").document.id();
    type Alter = Box<dyn Fn(&mut ScriptOutcome)>;
    let cases: Vec<Alter> = vec![
        Box::new(|o| o.mutation_count = 1),
        Box::new(move |o| o.invalidation = RenderInvalidation::full(root)),
        Box::new(|o| {
            o.invalidation.impact = crate::engine::invalidation::InvalidationImpact::PAINT
        }),
        Box::new(|o| o.invalidation.rebuild_style_rules = true),
        Box::new(move |o| o.invalidation.removed_nodes.push(root)),
        Box::new(|o| o.runtime_stopped = true),
        Box::new(|o| o.navigation_url = Some("about:blank".into())),
        Box::new(|o| o.viewport_scroll_y = Some(0.0)),
        Box::new(|o| o.viewport_wheel_delta_y = 1.0),
    ];
    for alter in cases {
        let mut outcome = pixels();
        alter(&mut outcome);
        assert!(!outcome.is_surface_repaint(), "{outcome:?}");
    }
}
