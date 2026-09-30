//! RFC 7845 sections 4.4/4.5: nonzero origins do not change pre-skip.

#[derive(Default)]
pub(super) struct Timeline {
    pub(super) raw_frames: u64,
    origin: Option<u64>,
    previous: Option<u64>,
    end: Option<u64>,
}

impl Timeline {
    pub(super) fn page(&mut self, samples: u64, granule: u64, eos: bool) -> Result<(), String> {
        if self.end.is_some() || samples == 0 || granule > i64::MAX as u64 {
            return Err("Ogg Opus audio page has an invalid granule position".into());
        }
        self.raw_frames = self
            .raw_frames
            .checked_add(samples)
            .ok_or("Ogg Opus raw duration overflows")?;
        let origin = match self.origin {
            Some(origin) => origin,
            None => {
                if !eos && granule < samples {
                    return Err("Ogg Opus initial granule is smaller than decoded samples".into());
                }
                let origin = granule.saturating_sub(samples);
                self.origin = Some(origin);
                origin
            }
        };
        let natural = origin
            .checked_add(self.raw_frames)
            .ok_or("Ogg Opus granule position overflows")?;
        if eos {
            if granule > natural || self.previous.is_some_and(|previous| granule < previous) {
                return Err(
                    "Ogg Opus EOS granule exceeds audio or precedes the previous page".into(),
                );
            }
            self.end = Some(
                granule
                    .checked_sub(origin)
                    .ok_or("Ogg Opus EOS precedes its initial origin")?,
            );
        } else if granule != natural {
            return Err("Ogg Opus page granules do not match decoded packet durations".into());
        }
        self.previous = Some(granule);
        Ok(())
    }

    pub(super) fn finish(&self, pre_skip: u16) -> Result<u64, String> {
        let end = self.end.ok_or("Ogg Opus has no completed EOS audio page")?;
        if end < u64::from(pre_skip) {
            return Err("Ogg Opus pre-skip exceeds the complete presentation".into());
        }
        Ok(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_eos_can_trim_or_encode_a_nonzero_origin_but_not_both_ambiguously() {
        let mut trimmed = Timeline::default();
        trimmed.page(1_920, 1_357, true).unwrap();
        assert_eq!(trimmed.finish(312).unwrap(), 1_357);
        let mut cropped = Timeline::default();
        cropped.page(1_920, 80_000, true).unwrap();
        assert_eq!(cropped.finish(312).unwrap(), 1_920);
    }

    #[test]
    fn rejects_backward_unset_short_initial_gap_and_excess_eos_granules() {
        for first in [0, 959, u64::MAX, i64::MAX as u64 + 1] {
            assert!(Timeline::default().page(960, first, false).is_err());
        }
        for next in [959, 1_919, 1_921, u64::MAX] {
            let mut timeline = Timeline::default();
            timeline.page(960, 960, false).unwrap();
            assert!(timeline.page(960, next, false).is_err());
        }
        for end in [959, 1_921] {
            let mut timeline = Timeline::default();
            timeline.page(960, 960, false).unwrap();
            assert!(timeline.page(960, end, true).is_err());
        }
        let mut timeline = Timeline::default();
        timeline.page(960, 311, true).unwrap();
        assert!(timeline.finish(312).is_err());
    }

    #[test]
    fn raw_duration_and_origin_arithmetic_are_checked() {
        let mut timeline = Timeline::default();
        timeline.page(960, i64::MAX as u64, false).unwrap();
        assert!(timeline.page(u64::MAX, 1_920, false).is_err());
        let mut timeline = Timeline::default();
        timeline.page(960, 960, false).unwrap();
        assert!(timeline.page(0, 960, true).is_err());
    }
}
