use super::*;

#[test]
fn disabled_profiling_does_not_sample_or_retain() {
    let mut profile = Profile::default();
    assert!(profile.start().is_none());
    profile.finish(Category::Draw, None, usize::MAX);
    assert!(profile.take(1).is_empty());
    assert_eq!(profile.peak_charged, 0);
}

#[test]
fn enabled_counts_drain_but_pressure_high_water_survives() {
    let mut profile = Profile::default();
    profile.enable(true, 20);
    profile.record(
        Category::Upload,
        Duration::from_millis(3),
        Some(Duration::from_millis(2)),
        100,
    );
    profile.record(
        Category::Upload,
        Duration::from_millis(1),
        Some(Duration::ZERO),
        80,
    );
    assert_eq!(
        profile.take(17),
        [
            "WebGL 17 native-owner charged high-water=100",
            "WebGL 17 native-owner storage/upload: 2 calls, 4.000 ms elapsed, 3.000 ms max, 2.000 ms owner-thread CPU",
        ]
    );
    assert_eq!(
        profile.take(17),
        ["WebGL 17 native-owner charged high-water=100"]
    );
    profile.enable(true, 1);
    assert_eq!(
        profile.peak_charged, 100,
        "idempotent enables keep the epoch"
    );
    profile.enable(false, 0);
    assert!(profile.take(17).is_empty());
    profile.enable(true, 30);
    assert_eq!(profile.peak_charged, 30);
}

#[test]
fn unavailable_cpu_is_not_reported_as_zero() {
    let mut profile = Profile::default();
    profile.enable(true, 0);
    profile.record(Category::Query, Duration::from_millis(1), None, 0);
    let text = profile.take(1).join("\n");
    assert!(text.contains("unavailable owner-thread CPU"));
    assert!(!text.contains("0.000 ms owner-thread CPU"));
}

#[test]
fn arbitrary_commands_never_become_diagnostic_labels() {
    let mut profile = Profile::default();
    profile.enable(true, 0);
    for command in [
        "private author source",
        "getSecret",
        "drawArrays",
        "texStorage3D",
        "compileShader",
    ] {
        profile.record(Category::command(command), Duration::ZERO, None, 0);
    }
    let rows = profile.take(1);
    assert!(rows.len() <= CATEGORIES + 1);
    let text = rows.join("\n");
    assert!(!text.contains("Secret"));
    assert!(!text.contains("private"));
    assert!(text.contains("validation/compile"));
}

#[test]
fn counters_saturate_without_affecting_work() {
    let mut profile = Profile::default();
    profile.enable(true, 0);
    profile.stats[Category::Draw as usize].calls = u64::MAX;
    profile.stats[Category::Draw as usize].elapsed = Duration::MAX;
    profile.record(Category::Draw, Duration::MAX, Some(Duration::MAX), 10);
    assert_eq!(profile.stats[Category::Draw as usize].calls, u64::MAX);
    assert_eq!(
        profile.stats[Category::Draw as usize].elapsed,
        Duration::MAX
    );
    assert_eq!(profile.peak_charged, 10);
}
