use super::*;

fn fixture() -> (Policy, Instant) {
    (Policy::new(1024), Instant::now())
}

#[test]
fn soft_pressure_does_not_change_the_hard_limit() {
    let (mut policy, now) = fixture();
    for (bytes, level) in [
        (0, Level::None),
        (639, Level::None),
        (640, Level::Moderate),
        (831, Level::Moderate),
        (832, Level::Critical),
        (2048, Level::Critical),
    ] {
        policy.observe(now, Some(bytes));
        assert_eq!(policy.advice().level, level, "{bytes}");
        assert_eq!(policy.limit, 1024);
    }
}

#[test]
fn recovery_requires_crossing_the_deadband() {
    let (mut policy, now) = fixture();
    for (bytes, level) in [
        (900, Level::Critical),
        (832, Level::Critical),
        (768, Level::Critical),
        (767, Level::Moderate),
        (640, Level::Moderate),
        (512, Level::Moderate),
        (511, Level::None),
    ] {
        policy.observe(now, Some(bytes));
        assert_eq!(policy.advice().level, level, "{bytes}");
    }
}

#[test]
fn missing_samples_never_claim_recovery() {
    let (mut policy, now) = fixture();
    policy.observe(now, Some(900));
    let before = policy.advice();
    for _ in 0..100 {
        policy.observe(now + Duration::from_secs(60), None);
    }
    assert_eq!(policy.advice().epoch, before.epoch);
    assert_eq!(policy.advice().level, Level::Critical);
    assert_eq!(policy.advice().private, 900);
}

#[test]
fn stable_pressure_is_not_a_per_frame_gc_loop() {
    let (mut policy, now) = fixture();
    policy.observe(now, Some(650));
    let epoch = policy.advice().epoch;
    for frame in 1..10_000 {
        policy.observe(now + Duration::from_millis(frame * 16), Some(650));
        assert_eq!(policy.advice().epoch, epoch);
    }
}

#[test]
fn growing_pressure_is_cooled_but_escalation_is_immediate() {
    let (mut policy, now) = fixture();
    policy.observe(now, Some(650));
    let epoch = policy.advice().epoch;
    policy.observe(now + Duration::from_secs(1), Some(715));
    assert_eq!(policy.advice().epoch, epoch);
    policy.observe(now + PULSE_INTERVAL, Some(715));
    assert_eq!(policy.advice().epoch, epoch + 1);
    policy.observe(now + PULSE_INTERVAL + Duration::from_millis(1), Some(900));
    assert_eq!(policy.advice().epoch, epoch + 2);
    assert_eq!(policy.advice().level, Level::Critical);
}

#[test]
fn a_small_growth_or_clock_regression_does_not_force_a_pulse() {
    let (mut policy, now) = fixture();
    policy.observe(now, Some(650));
    let epoch = policy.advice().epoch;
    policy.observe(now - PULSE_INTERVAL, Some(715));
    policy.observe(now + PULSE_INTERVAL, Some(713));
    assert_eq!(policy.advice().epoch, epoch);
}

#[test]
fn unusable_zero_budget_and_extreme_values_do_not_overflow() {
    let mut policy = Policy::new(0);
    policy.observe(Instant::now(), Some(usize::MAX));
    assert_eq!(policy.advice().epoch, 0);
    let mut policy = Policy::new(usize::MAX);
    policy.observe(Instant::now(), Some(usize::MAX));
    assert_eq!(policy.advice().level, Level::Critical);
}
