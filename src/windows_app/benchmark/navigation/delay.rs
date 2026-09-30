//! The first action may use a settled-stage delay; later actions keep their spacing.
use std::time::Duration;

pub(super) fn next_action_delay(initial: &mut Option<Duration>, spacing: Duration) -> Duration {
    initial.take().unwrap_or(spacing)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settled_first_action_does_not_multiply_subsequent_wheel_spacing() {
        let mut initial = Some(Duration::from_secs(12));
        assert_eq!(
            next_action_delay(&mut initial, Duration::from_secs(1)),
            Duration::from_secs(12)
        );
        assert_eq!(
            next_action_delay(&mut initial, Duration::from_secs(1)),
            Duration::from_secs(1)
        );
        assert_eq!(
            next_action_delay(&mut None, Duration::from_secs(1)),
            Duration::from_secs(1)
        );
    }
}
