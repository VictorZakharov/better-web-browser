use super::GraphPcmHealth;
use std::time::Duration;

pub(super) fn extrapolate(
    mut health: GraphPcmHealth,
    sample_rate: u32,
    elapsed: Duration,
) -> GraphPcmHealth {
    // Pending transport chunks cannot advance the device clock. A priming
    // voice has not started yet; an empty voice stops at its submitted horizon.
    if health.playing {
        let frames = (elapsed.as_secs_f64() * f64::from(sample_rate)) as u64;
        health.played_frames = health
            .played_frames
            .saturating_add(frames)
            .min(health.submitted_frames);
    }
    health
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_clock_advances_only_a_started_voice_and_never_pending_transport_bytes() {
        let mut health = GraphPcmHealth {
            submitted_frames: 896 * 4,
            played_frames: 128,
            underruns: 2,
            playing: false,
        };
        assert_eq!(extrapolate(health, 48_000, Duration::from_secs(1)), health);
        health.playing = true;
        let progressed = extrapolate(health, 48_000, Duration::from_millis(10));
        assert_eq!(progressed.played_frames, 608);
        assert_eq!(progressed.underruns, 2);
        assert_eq!(
            extrapolate(health, 48_000, Duration::MAX).played_frames,
            3584
        );
    }
}
