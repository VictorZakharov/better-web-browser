use super::*;

const FIXTURE: &str = include_str!("../../../../tests/canvas/clipped-solid-paint.js");
const MEMBERSHIP: &str = include_str!("../../../../tests/canvas/clip-membership.js");

#[test]
fn native_and_scalar_clips_match_independent_membership_at_the_admission_boundary() {
    let (_, outcome) = execute_html(&format!(
        "<script>{MEMBERSHIP};const make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};if(testCanvasClipMembership(make)!==144)throw Error('missing cases');</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_clip_membership_uses_worker_owned_paths_and_mask_storage() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.test/clip-membership.js",
        &format!(
            "{MEMBERSHIP};postMessage(testCanvasClipMembership((w,h)=>new OffscreenCanvas(w,h)));"
        ),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["144"]);
}

#[test]
fn clipped_native_paint_matches_scalar_bytes_with_holes_intersections_and_saved_regions() {
    for clip in 0..4 {
        let (_, outcome) = execute_html(&format!(
            "<script>{FIXTURE};const make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};if(testClippedSolidPaint(make,{clip})!==60)throw Error('missing cases');</script>"
        ));
        assert!(
            outcome.errors.is_empty(),
            "clip {clip}: {:?}",
            outcome.errors
        );
    }
}

#[test]
fn worker_clipped_native_paint_uses_the_same_packed_clip_and_owned_region() {
    for clip in 0..4 {
        let (_, outcome) = WorkerRuntime::start(
            "https://example.test/clipped-paint.js",
            &format!(
                "{FIXTURE};postMessage(testClippedSolidPaint((w,h)=>new OffscreenCanvas(w,h),{clip}));"
            ),
            "",
            ScriptKind::Classic,
            std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
        );
        assert!(
            outcome.errors.is_empty(),
            "clip {clip}: {:?}",
            outcome.errors
        );
        assert_eq!(outcome.messages, ["60"]);
    }
}
