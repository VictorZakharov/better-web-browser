use super::*;

fn sample(private: usize, working_set: usize, peak_working_set: usize) -> ProcessSample {
    ProcessSample {
        memory_available: true,
        private_memory: private,
        working_set,
        peak_working_set,
        ..Default::default()
    }
}

#[test]
fn no_observation_is_unknown_not_a_successful_zero_memory_sample() {
    let evidence = RendererMemoryEvidence::from_sample(ProcessSample::default());
    assert!(!evidence.current_sample_available);
    assert_eq!(evidence.successful_samples, 0);
    assert_eq!(evidence.last_private_bytes, None);
    assert_eq!(evidence.observed_peak_private_bytes, None);
    assert_eq!(evidence.retained_peak_working_set_bytes, None);
}

#[test]
fn a_successful_zero_sample_remains_distinguishable_from_no_observation() {
    let evidence = RendererMemoryEvidence::from_sample(sample(0, 0, 0));
    assert!(evidence.current_sample_available);
    assert_eq!(evidence.successful_samples, 1);
    assert_eq!(evidence.last_private_bytes, Some(0));
    assert_eq!(evidence.observed_peak_private_bytes, Some(0));
    assert_eq!(evidence.retained_peak_working_set_bytes, Some(0));
}

#[test]
fn private_high_water_is_observed_and_working_set_retains_the_os_peak() {
    let mut evidence = RendererMemoryEvidence::from_sample(sample(800, 600, 700));
    evidence.observe(sample(400, 450, 700));
    assert_eq!(evidence.last_private_bytes, Some(400));
    assert_eq!(evidence.last_working_set_bytes, Some(450));
    assert_eq!(evidence.observed_peak_private_bytes, Some(800));
    assert_eq!(evidence.retained_peak_working_set_bytes, Some(700));
    evidence.observe(sample(900, 800, 750));
    assert_eq!(evidence.observed_peak_private_bytes, Some(900));
    assert_eq!(evidence.retained_peak_working_set_bytes, Some(800));
    assert_eq!(evidence.successful_samples, 3);
}

#[test]
fn failed_queries_and_terminal_unavailability_preserve_last_known_evidence() {
    let mut evidence = RendererMemoryEvidence::from_sample(sample(900, 800, 950));
    for _ in 0..3 {
        evidence.observe(ProcessSample::default());
        assert!(!evidence.current_sample_available);
        assert_eq!(evidence.last_private_bytes, Some(900));
        assert_eq!(evidence.observed_peak_private_bytes, Some(900));
        assert_eq!(evidence.retained_peak_working_set_bytes, Some(950));
        assert_eq!(evidence.successful_samples, 1);
    }
    evidence.observe(sample(300, 200, 950));
    assert!(evidence.current_sample_available);
    assert_eq!(evidence.last_private_bytes, Some(300));
    evidence.unavailable();
    assert!(!evidence.current_sample_available);
    assert_eq!(evidence.observed_peak_private_bytes, Some(900));
    assert_eq!(evidence.last_private_bytes, Some(300));
    assert_eq!(evidence.successful_samples, 2);
}

#[test]
fn evidence_is_per_renderer_and_copying_a_snapshot_does_not_reset_the_live_observer() {
    let mut first = RendererMemoryEvidence::from_sample(sample(900, 800, 950));
    let second = RendererMemoryEvidence::from_sample(sample(40, 30, 50));
    let mut historical = first;
    historical.unavailable();
    first.observe(sample(500, 400, 950));
    assert_eq!(historical.last_private_bytes, Some(900));
    assert!(!historical.current_sample_available);
    assert!(first.current_sample_available);
    assert_eq!(first.last_private_bytes, Some(500));
    assert_eq!(second.observed_peak_private_bytes, Some(40));
}

#[test]
fn counters_and_extreme_byte_values_never_wrap_or_create_unbounded_history() {
    let mut evidence = RendererMemoryEvidence {
        successful_samples: u64::MAX,
        ..Default::default()
    };
    for _ in 0..10_000 {
        evidence.observe(sample(usize::MAX, usize::MAX, usize::MAX));
    }
    assert_eq!(evidence.successful_samples, u64::MAX);
    assert_eq!(evidence.observed_peak_private_bytes, Some(usize::MAX));
    assert_eq!(evidence.retained_peak_working_set_bytes, Some(usize::MAX));
    assert!(std::mem::size_of::<RendererMemoryEvidence>() <= 128);
}
