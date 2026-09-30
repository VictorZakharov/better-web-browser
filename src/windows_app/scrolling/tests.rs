use super::*;

#[test]
fn fractional_css_wheel_distance_is_retained_across_inputs() {
    let mut animation = ScrollAnimation::default();
    let pixels: i32 = (0..16)
        .map(|_| animation.consume_css_delta(0.25, 1.25))
        .sum();
    assert_eq!(pixels, 5);
    let reversed: i32 = (0..16)
        .map(|_| animation.consume_css_delta(-0.25, 1.25))
        .sum();
    assert_eq!(reversed, -5);
    assert_eq!(animation.pixel_remainder, 0.0);
}

#[test]
fn unchanged_and_clamped_targets_do_not_introduce_new_owned_motion() {
    let mut animation = ScrollAnimation {
        target: Some(100),
        ..ScrollAnimation::default()
    };
    for distance in [0, 100] {
        assert!(!animation.plan_distance(20, distance, 100).introduces_motion);
    }
    let reversed = animation.plan_distance(20, -10, 100);
    assert!(reversed.introduces_motion);
    assert_eq!(reversed.target, 10);
    animation.target = None;
    assert!(!animation.plan_distance(100, 50, 100).introduces_motion);
}

#[test]
fn fractional_distance_keeps_its_remainder_without_owning_an_old_animation_tick() {
    let mut animation = ScrollAnimation {
        target: Some(100),
        ..ScrollAnimation::default()
    };
    let distance = animation.consume_css_delta(0.25, 1.25);
    assert_eq!(distance, 0);
    assert!(!animation.plan_distance(20, distance, 100).introduces_motion);
    assert_eq!(animation.consume_css_delta(0.25, 1.25), 1);
    assert!(animation.plan_distance(20, 1, 200).introduces_motion);
}

#[test]
fn response_curve_advances_without_overshooting() {
    let progress =
        1.0 - (-(FRAME_TIMER_INTERVAL_MS as f64 / 1_000.0) / RESPONSE_TIME.as_secs_f64()).exp();
    let step = (126.0 * progress).round() as i32;
    assert!((29..=31).contains(&step));
    assert!(step < 126);
}

#[test]
fn high_resolution_wheel_deltas_accumulate_to_one_notch() {
    let mut animation = ScrollAnimation::default();
    assert_eq!(animation.consume_wheel_delta(30), 0);
    assert_eq!(animation.consume_wheel_delta(30), 0);
    assert_eq!(animation.consume_wheel_delta(30), 0);
    assert_eq!(animation.consume_wheel_delta(30), 1);
    assert_eq!(animation.wheel_delta_remainder, 0);
}

#[test]
fn reversal_discards_long_backlog_and_first_frame_moves_in_the_new_direction() {
    for (position, pending, distance, expected) in [
        (20, 100, -10, 10),
        (100, 20, 10, 110),
        (1_000, 9_000, -126, 874),
        (9_000, 1_000, 126, 9_126),
    ] {
        let animation = ScrollAnimation {
            target: Some(pending),
            ..ScrollAnimation::default()
        };
        let request = animation.plan_distance(position, distance, 10_000);
        assert_eq!(request.target, expected);
        assert!(request.introduces_motion);
        let next = next_scroll_position(position, request.target, Duration::from_millis(15));
        assert_eq!((next - position).signum(), distance.signum());
        assert_eq!(
            animation.target,
            Some(pending),
            "planning is side-effect free"
        );
    }
}

#[test]
fn same_direction_accumulates_and_zero_does_not_cancel_pending_motion() {
    for (position, pending, distance, expected) in [(20, 100, 10, 110), (100, 20, -10, 10)] {
        let animation = ScrollAnimation {
            target: Some(pending),
            ..ScrollAnimation::default()
        };
        assert_eq!(
            animation.plan_distance(position, distance, 200).target,
            expected
        );
        let zero = animation.plan_distance(position, 0, 200);
        assert_eq!(zero.target, pending);
        assert!(!zero.introduces_motion);
    }
    let animation = ScrollAnimation::default();
    assert_eq!(animation.plan_distance(100, -10, 200).target, 90);
}

#[test]
fn repeated_alternations_never_have_to_repay_the_previous_target() {
    let mut animation = ScrollAnimation {
        target: Some(9_000),
        ..ScrollAnimation::default()
    };
    let mut position = 5_000;
    for distance in [-10, 10, -10, 10, -1, 1] {
        let request = animation.plan_distance(position, distance, 10_000);
        assert_eq!(request.target, position + distance);
        assert!(request.introduces_motion);
        animation.target = Some(request.target);
        let next = next_scroll_position(position, request.target, Duration::from_millis(15));
        assert_eq!((next - position).signum(), distance.signum());
        position = next;
    }
}

#[test]
fn reversal_clamps_to_actual_edges_without_owning_old_motion() {
    for (position, pending, distance, expected, motion) in [
        (0, 100, -10, 0, false),
        (200, 100, 10, 200, false),
        (20, 100, -80, 0, true),
        (180, 100, 80, 200, true),
    ] {
        let animation = ScrollAnimation {
            target: Some(pending),
            ..ScrollAnimation::default()
        };
        let request = animation.plan_distance(position, distance, 200);
        assert_eq!(request.target, expected);
        assert_eq!(request.introduces_motion, motion);
        let next = next_scroll_position(position, request.target, Duration::from_millis(15));
        assert_eq!(next == position, !motion);
    }
}

#[test]
fn extreme_reverse_distances_saturate_and_the_first_frame_does_not_overflow() {
    for (position, pending, distance, expected) in [
        (i32::MAX - 1, i32::MAX, i32::MIN, 0),
        (1, 0, i32::MAX, i32::MAX),
    ] {
        let animation = ScrollAnimation {
            target: Some(pending),
            ..ScrollAnimation::default()
        };
        let request = animation.plan_distance(position, distance, i32::MAX);
        assert_eq!(request.target, expected);
        let next = next_scroll_position(position, request.target, Duration::from_millis(50));
        assert_eq!((next - position).signum(), distance.signum());
        assert!((position.min(expected)..=position.max(expected)).contains(&next));
    }
}

#[test]
fn css_subpixel_reversal_is_detected_before_rounding_and_discards_old_residues() {
    for direction in [-1, 1] {
        let mut animation = ScrollAnimation {
            target: Some(500 + direction * 100),
            pixel_remainder: direction as f64 * 0.49,
            wheel_delta_remainder: -direction * 119,
            ..ScrollAnimation::default()
        };
        let delta = -direction as f32 * 0.125;
        // The CSS route uses the original delta's sign before consume_css_delta.
        assert!(animation.reverses_pending(500, if delta > 0.0 { 1 } else { -1 }));
        animation.discard_input_remainders();
        assert_eq!(animation.wheel_delta_remainder, 0);
        assert_eq!(animation.consume_css_delta(delta, 1.25), 0);
        assert_eq!(animation.pixel_remainder, delta as f64 * 1.25);
        let distance = animation.consume_css_delta(-direction as f32 * 0.375, 1.25);
        assert_eq!(distance, -direction);
        assert_eq!(
            animation.plan_distance(500, distance, 2_000).target,
            500 - direction
        );
        assert_eq!(
            distance as f64 + animation.pixel_remainder,
            -direction as f64 * 0.5 * 1.25
        );
    }
}

#[test]
fn raw_subnotch_reversal_is_detected_before_accumulation_and_discards_old_residues() {
    for direction in [-1, 1] {
        let mut animation = ScrollAnimation {
            target: Some(500 + direction * 100),
            wheel_delta_remainder: -direction * 119,
            pixel_remainder: direction as f64 * 0.49,
            ..ScrollAnimation::default()
        };
        let delta: i32 = direction * 30;
        // Win32 positive raw deltas scroll upward, opposite to CSS coordinates.
        assert!(animation.reverses_pending(500, -delta.signum()));
        animation.discard_input_remainders();
        assert_eq!(animation.pixel_remainder, 0.0);
        for _ in 0..3 {
            assert_eq!(animation.consume_wheel_delta(delta), 0);
        }
        assert_eq!(animation.wheel_delta_remainder, direction * 90);
        let notches = animation.consume_wheel_delta(delta);
        assert_eq!(notches, direction);
        assert_eq!(animation.wheel_delta_remainder, 0);
        let request = animation.plan_distance(500, -notches * WHEEL_STEP_DIP, 2_000);
        assert_eq!(request.target, 500 - direction * WHEEL_STEP_DIP);
        let next = next_scroll_position(500, request.target, Duration::from_millis(15));
        assert_eq!((next - 500).signum(), -direction);
    }
}

#[test]
fn zero_same_direction_and_finished_targets_do_not_request_cancellation() {
    for direction in [-1, 1] {
        let animation = ScrollAnimation {
            target: Some(500 + direction * 100),
            wheel_delta_remainder: -direction * 119,
            pixel_remainder: direction as f64 * 0.49,
            ..ScrollAnimation::default()
        };
        assert!(!animation.reverses_pending(500, 0));
        assert!(!animation.reverses_pending(500, direction));
        assert!(animation.reverses_pending(500, -direction));
        assert_eq!(animation.pixel_remainder, direction as f64 * 0.49);
        assert_eq!(animation.wheel_delta_remainder, -direction * 119);
        assert!(!animation.reverses_pending(500 + direction * 100, -direction));
    }
    assert!(!ScrollAnimation::default().reverses_pending(500, -1));
}
