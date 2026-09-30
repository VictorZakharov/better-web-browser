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
fn active_target_updates_preserve_the_frame_clock_and_timer() {
    for direction in [-1, 1] {
        let started = Instant::now();
        let mut animation = ScrollAnimation {
            target: Some(1_000 + direction * WHEEL_STEP_DIP),
            last_frame: Some(started),
            ..ScrollAnimation::default()
        };
        for distance in [direction * WHEEL_STEP_DIP, direction, 0] {
            let request = animation.plan_distance(1_000, distance, 2_000);
            assert!(
                !animation.retarget(request.target),
                "extending an active target must not restart its timer"
            );
            assert_eq!(animation.last_frame, Some(started));
        }
        assert_eq!(
            animation.frame_elapsed(started + Duration::from_millis(23)),
            Duration::from_millis(23),
            "the next frame consumes wall time rather than a synthetic input tick"
        );
    }
}

#[test]
fn input_bursts_extend_one_animation_with_frames_between_and_after_them() {
    let started = Instant::now();
    let mut animation = ScrollAnimation::default();
    assert!(animation.retarget(WHEEL_STEP_DIP));
    let initial_elapsed = animation.frame_elapsed(started);
    let mut position = next_scroll_position(0, WHEEL_STEP_DIP, initial_elapsed);
    assert!((29..=31).contains(&position));

    for frame in 1..=3 {
        let previous_frame = animation.last_frame;
        // Several inputs arrive before the next scheduled frame. Only their
        // target changes; frame progression stays owned by the existing timer.
        for _ in 0..3 {
            let request = animation.plan_distance(position, WHEEL_STEP_DIP, 10_000);
            assert!(request.introduces_motion);
            assert!(!animation.retarget(request.target));
            assert_eq!(animation.last_frame, previous_frame);
        }
        let now = started + Duration::from_millis(frame * 15);
        let elapsed = animation.frame_elapsed(now);
        assert_eq!(elapsed, Duration::from_millis(15));
        let next = next_scroll_position(position, animation.target.unwrap(), elapsed);
        assert!(next > position, "frames must continue between input bursts");
        assert!(next < animation.target.unwrap());
        position = next;
    }

    let target = animation.target.unwrap();
    assert_eq!(target, WHEEL_STEP_DIP * 10);
    for frame in 4..=40 {
        let elapsed = animation.frame_elapsed(started + Duration::from_millis(frame * 15));
        let next = next_scroll_position(position, target, elapsed);
        assert!((position..=target).contains(&next));
        position = next;
    }
    assert_eq!(
        position, target,
        "timer frames finish all accumulated travel"
    );
}

#[test]
fn idle_and_reversed_gestures_start_with_a_prompt_first_frame() {
    let started = Instant::now();
    let mut animation = ScrollAnimation::default();
    assert!(animation.retarget(9_000));
    assert_eq!(animation.last_frame, None);
    let mut position = 5_000;
    let first_elapsed = animation.frame_elapsed(started);
    assert_eq!(first_elapsed, Duration::from_millis(15));
    position = next_scroll_position(position, 9_000, first_elapsed);

    assert!(animation.reverses_pending(position, -1));
    assert!(animation.cancel(true).was_active);
    assert_eq!(animation.last_frame, None);
    let request = animation.plan_distance(position, -WHEEL_STEP_DIP, 10_000);
    assert_eq!(request.target, position - WHEEL_STEP_DIP);
    assert!(animation.retarget(request.target));
    let reverse_elapsed = animation.frame_elapsed(started + Duration::from_millis(1));
    assert_eq!(reverse_elapsed, Duration::from_millis(15));
    assert!(next_scroll_position(position, request.target, reverse_elapsed) < position);

    assert!(animation.cancel(true).was_active);
    assert!(!animation.cancel(true).was_active);
    assert_eq!(animation.target, None);
    assert_eq!(animation.last_frame, None);
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

#[test]
fn suspended_gesture_returns_with_a_fresh_same_direction_animation() {
    for direction in [-1, 1] {
        let now = Instant::now();
        let mut animation = ScrollAnimation {
            target: Some(1_000 + direction * WHEEL_STEP_DIP),
            last_frame: Some(now),
            wheel_delta_remainder: direction * 119,
            pixel_remainder: direction as f64 * 0.25,
        };
        let cancellation = animation.cancel(true);
        animation.discard_input_remainders();
        assert!(cancellation.was_active && cancellation.stop_timer);
        assert_eq!(animation.target, None);
        assert_eq!(animation.last_frame, None);
        assert_eq!(animation.wheel_delta_remainder, 0);
        assert_eq!(animation.pixel_remainder, 0.0);
        let request = animation.plan_distance(1_000, direction * WHEEL_STEP_DIP, 2_000);
        assert_eq!(request.target, 1_000 + direction * WHEEL_STEP_DIP);
        assert!(animation.retarget(request.target));
        let elapsed = animation.frame_elapsed(now + Duration::from_secs(1));
        assert_eq!(elapsed, Duration::from_millis(15));
        assert_eq!(
            next_scroll_position(1_000, request.target, elapsed).cmp(&1_000),
            direction.cmp(&0)
        );
    }
}

#[test]
fn background_defaults_and_absolute_cancellation_cannot_take_the_window_timer() {
    let now = Instant::now();
    let mut background = ScrollAnimation {
        target: Some(900),
        last_frame: Some(now),
        ..ScrollAnimation::default()
    };
    // Cancellation is shared by accepted background wheel defaults and script
    // absolute scroll requests. Neither may stop the foreground HWND timer.
    let cancellation = background.cancel(false);
    assert!(cancellation.was_active);
    assert!(!cancellation.stop_timer);
    let request = immediate_scroll_request(500, WHEEL_STEP_DIP, 1_000);
    assert_eq!(
        request.target, 626,
        "late input starts at the actual position"
    );
    assert_eq!(background.target, None);
    assert_eq!(background.last_frame, None);
    let mut position = 500;
    for _ in 0..16 {
        let distance = background.consume_css_delta(0.25, 1.25);
        assert!(!background.cancel(false).stop_timer);
        position = immediate_scroll_request(position, distance, 1_000).target;
    }
    assert_eq!(
        position, 505,
        "direct defaults retain fractional CSS distance"
    );
    for (position, distance, target) in [(0, -126, 0), (1_000, 126, 1_000), (990, 126, 1_000)] {
        let request = immediate_scroll_request(position, distance, 1_000);
        assert_eq!(request.target, target);
        assert_eq!(request.introduces_motion, target != position);
    }
}
