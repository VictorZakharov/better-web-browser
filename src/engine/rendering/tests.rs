use super::*;

#[test]
fn high_refresh_and_fractional_rates_have_no_sixty_or_144_hz_ceiling() {
    for frequency in [60, 120, 144, 165, 200, 240, 360, 500, 1000] {
        let rate = RefreshRate::new(frequency, 1).unwrap();
        assert_eq!(
            rate.next_after(Duration::ZERO).as_nanos(),
            1_000_000_000_u128.div_ceil(u128::from(frequency))
        );
    }
    let fractional = RefreshRate::new(60000, 1001).unwrap();
    assert_eq!(
        fractional.next_after(Duration::ZERO),
        Duration::from_nanos(16_683_334)
    );
    assert_eq!(
        fractional.next_after(Duration::from_secs(1001)),
        Duration::from_nanos(1_001_016_683_334)
    );
}

#[test]
fn grid_does_not_accumulate_rounding_or_replay_missed_opportunities() {
    let rate = RefreshRate::new(240, 1).unwrap();
    let first = rate.next_after(Duration::ZERO);
    assert_eq!(rate.next_after(first), Duration::from_nanos(8_333_334));
    assert_eq!(
        rate.next_after(Duration::from_millis(37)),
        Duration::from_nanos(37_500_000)
    );
    assert_eq!(
        rate.next_after(Duration::from_nanos(37_500_000)),
        Duration::from_nanos(41_666_667)
    );
    assert_eq!(
        rate.next_after(Duration::from_secs(1000)),
        Duration::from_nanos(1_000_004_166_667)
    );
}

#[test]
fn invalid_rates_and_extreme_elapsed_times_are_bounded() {
    assert!(RefreshRate::new(0, 1).is_none());
    assert!(RefreshRate::new(60, 0).is_none());
    for rate in [
        RefreshRate::new(u32::MAX, 1).unwrap(),
        RefreshRate::new(1, u32::MAX).unwrap(),
    ] {
        assert!(rate.next_after(Duration::ZERO) > Duration::ZERO);
        assert_eq!(rate.next_after(Duration::MAX), Duration::MAX);
    }
}
