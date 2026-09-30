//! Backpressure may combine same-direction distance, never erase a reversal.
use super::*;

fn wheel(delta: f32) -> RuntimeReport {
    RuntimeReport {
        viewport_wheel_delta_y: delta,
        ..RuntimeReport::default()
    }
}

#[test]
fn reverse_wheels_keep_both_runtime_updates_in_delivery_order() {
    for (forward, reverse) in [(600.0, -126.0), (-600.0, 126.0), (126.0, -126.0)] {
        let first = update(wheel(forward));
        let next = update(wheel(reverse));
        let (retained, remaining) = first.clone().coalesce(next.clone()).unwrap();
        assert_eq!(retained, first);
        assert_eq!(remaining, Some(next));
    }
}

#[test]
fn reverse_wheels_keep_both_presentations_with_their_own_snapshots() {
    for (forward, reverse) in [(600.0, -126.0), (-600.0, 126.0), (126.0, -126.0)] {
        let mut first = sample();
        first.runtime = wheel(forward);
        first.title = "forward snapshot".into();
        let mut next = sample();
        next.revision = first.revision + 1;
        next.runtime = wheel(reverse);
        next.title = "reverse snapshot".into();
        let (retained, remaining) = first.clone().coalesce(next.clone()).unwrap();
        assert_eq!(retained.revision, first.revision);
        assert_eq!(retained.runtime.viewport_wheel_delta_y, forward);
        assert_eq!(retained.title, "forward snapshot");
        let remaining = remaining.expect("reversal must retain its own presentation");
        assert_eq!(remaining.revision, next.revision);
        assert_eq!(remaining.runtime.viewport_wheel_delta_y, reverse);
        assert_eq!(remaining.title, "reverse snapshot");
    }
}

#[test]
fn same_direction_zero_and_superseding_absolute_scroll_still_compact() {
    for (forward, next_delta, absolute) in [
        (126.0, 126.0, None),
        (-126.0, -40.0, None),
        (126.0, 0.0, None),
        (0.0, -126.0, None),
        (126.0, -0.0, None),
        (126.0, -126.0, Some(800.0)),
    ] {
        let first = wheel(forward);
        let mut next = wheel(next_delta);
        next.viewport_scroll_y = absolute;
        let (merged, remaining) = update(first).coalesce(update(next)).unwrap();
        assert!(remaining.is_none());
        assert_eq!(merged.runtime.viewport_scroll_y, absolute);
        assert_eq!(
            merged.runtime.viewport_wheel_delta_y,
            if absolute.is_some() {
                next_delta
            } else {
                forward + next_delta
            }
        );
    }
}
