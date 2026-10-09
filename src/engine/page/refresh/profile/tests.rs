use super::*;
use crate::engine::invalidation::{MutationKind, RenderInvalidation};
use crate::engine::page::Page;

#[test]
fn phase_clock_is_disabled_by_default_and_draining_never_enables_it() {
    let mut profile = Profile::default();
    assert!(profile.start().is_none());
    profile.finish(Phase::Cascade, None);
    assert!(profile.take().is_none());
    assert!(profile.start().is_none());
    profile.enable(true);
    assert!(profile.take().is_none());
    profile.finish(Phase::Cascade, profile.start());
    let row = profile.take().unwrap();
    assert!(row.starts_with("resource checkpoint phases: discovery "));
    assert!(row.contains(", cascade ") && row.contains(", SVG "));
    assert!(row.ends_with("diagnostic overhead applies"));
    assert!(row.len() < 512);
    assert!(profile.take().is_none());
    profile.finish(Phase::Fonts, profile.start());
    profile.enable(false);
    assert!(profile.take().is_none());
    assert!(profile.start().is_none());
}

#[test]
fn repeated_phases_accumulate_only_inside_the_current_checkpoint() {
    let mut profile = Profile::default();
    profile.enable(true);
    for _ in 0..2 {
        // Owned synthetic starts avoid sleeps and platform-duration thresholds.
        profile.finish(Phase::Fonts, Some(Instant::now() - Duration::from_secs(1)));
    }
    assert!(profile.elapsed[Phase::Fonts as usize] >= Duration::from_secs(2));
    profile.reset();
    assert!(profile.elapsed.iter().all(Duration::is_zero));
    assert!(profile.take().is_none());
}

#[test]
fn full_and_failed_retained_checkpoints_report_current_phases_not_old_timings() {
    let mut page = Page::parse_scripted(
        "<style>div{color:red}</style><div id=target>text</div>",
        "https://example.test/secret-path?private-query",
    );
    page.set_resource_profiling(true);
    page.refresh_resources_for_viewport(800., 600.);
    let row = page.take_resource_diagnostic().unwrap();
    assert!(!row.contains("secret") && !row.contains("private-query") && !row.contains("target"));
    assert!(page.take_resource_diagnostic().is_none());
    let target = page.dom.elements_named("div").next().unwrap();
    target.set_attr("style", "color:blue;background-image:url(icon.png)");
    let dirty = RenderInvalidation {
        roots: vec![target.id()],
        impact: MutationKind::Attribute("style").impact(),
        mutation_count: 1,
        ..Default::default()
    };
    let (_, unchanged) = page.refresh_presentation_styles(800., 600., &dirty);
    assert!(!unchanged);
    assert!(page.take_resource_diagnostic().is_some());
    let (_, unchanged) = page.refresh_presentation_styles(800., 600., &dirty);
    assert!(unchanged);
    let row = page.take_resource_diagnostic().unwrap();
    assert!(row.contains("discovery 0.000 ms") && row.contains("SVG 0.000 ms"));
    assert!(page.take_resource_diagnostic().is_none());
}

#[test]
fn layout_snapshots_have_independent_disabled_resource_profiles() {
    let mut page = Page::parse("<p>text</p>", "https://example.test/");
    page.set_resource_profiling(true);
    page.refresh_resources_for_viewport(800., 600.);
    let mut snapshot = page.layout_snapshot();
    snapshot.synchronize_layout_snapshot(&page);
    snapshot.refresh_resources_for_viewport(800., 600.);
    assert!(snapshot.take_resource_diagnostic().is_none());
    assert!(page.take_resource_diagnostic().is_some());
}
