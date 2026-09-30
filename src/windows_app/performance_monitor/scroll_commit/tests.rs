use super::*;

fn timings(milliseconds: u64) -> ScrollCommitTimings {
    ScrollCommitTimings {
        total: Duration::from_millis(milliseconds),
        controls: Duration::from_micros(500),
        accessibility: Duration::from_micros(250),
        paint: Duration::from_micros(250),
    }
}

#[test]
fn commit_costs_are_totals_with_independent_count_percentile_and_maximum() {
    let start = Instant::now();
    let mut commits = ScrollCommits::default();
    for index in 1..=20 {
        commits.record(
            start + Duration::from_millis(index),
            timings(index),
            index as usize,
            index % 2 == 0,
        );
    }
    let snapshot = commits.snapshot(start + Duration::from_millis(20));
    assert_eq!(snapshot.count, 20);
    assert_eq!(snapshot.total, Duration::from_millis(210));
    assert_eq!(snapshot.p95, Duration::from_millis(19));
    assert_eq!(snapshot.maximum, Duration::from_millis(20));
    assert_eq!(snapshot.controls, Duration::from_millis(10));
    assert_eq!(snapshot.accessibility, Duration::from_millis(5));
    assert_eq!(snapshot.paint, Duration::from_millis(5));
    assert_eq!(snapshot.maximum_native_controls, 20);
    assert_eq!(snapshot.accessibility_active, 10);
}

#[test]
fn commit_samples_expire_without_new_activity_and_remain_memory_bounded() {
    let start = Instant::now();
    let mut commits = ScrollCommits::default();
    for index in 0..=MAX_FRAME_SAMPLES {
        commits.record(start, timings(index as u64), 0, false);
    }
    assert_eq!(commits.samples.len(), MAX_FRAME_SAMPLES);
    assert_eq!(
        commits.snapshot(start + FRAME_WINDOW).count,
        MAX_FRAME_SAMPLES
    );
    assert_eq!(
        commits
            .snapshot(start + FRAME_WINDOW + Duration::from_millis(1))
            .count,
        0
    );
    commits.record(
        start + FRAME_WINDOW + Duration::from_millis(1),
        timings(1),
        2,
        true,
    );
    assert_eq!(commits.samples.len(), 1);
}

#[test]
fn slow_commits_report_contributors_at_the_33_ms_boundary() {
    let mut sample = timings(32);
    sample.total = SLOW_COMMIT - Duration::from_micros(1);
    assert!(slow_incident(sample, 2, false).is_none());
    sample.total = SLOW_COMMIT;
    let label = slow_incident(sample, 2, true).unwrap();
    assert!(label.contains("commit 33.0 ms"));
    assert!(label.contains("control sync 0.5"));
    assert!(label.contains("accessibility bounds 0.2"));
    assert!(label.contains("other 32.0 ms"));
    assert!(label.contains("native controls 2, accessibility active true"));
}

#[test]
fn diagnostic_scope_keeps_commit_costs_distinct_from_paints_and_wheel_latency() {
    let lines = ScrollCommitSnapshot::default().diagnostic_lines();
    assert!(lines[0].contains("0 moving commits"));
    assert!(lines[1].contains("totals"));
    assert!(lines[2].contains("accessibility active 0/0"));
    assert!(lines[3].contains("subsets, not wheel-to-response latency"));
}
