//! Observed native animation commits, distinct from first owning retained paint.
use super::{DocumentId, Duration, Instant};

const MAX_FRAMES: usize = 512;

#[derive(Default)]
pub(super) struct AnimationTrace {
    started: Option<Instant>,
    frames: Vec<Frame>,
    omitted: u64,
}

struct Frame {
    document: DocumentId,
    offset: Duration,
    y: f64,
    initial: bool,
}

impl AnimationTrace {
    pub(super) fn begin(&mut self, now: Instant) {
        self.started.get_or_insert(now);
    }

    pub(super) fn offset(&self, now: Instant) -> Option<Duration> {
        self.started
            .map(|started| now.saturating_duration_since(started))
    }

    pub(super) fn record(&mut self, document: DocumentId, y: f64, initial: bool, now: Instant) {
        let Some(offset) = self.offset(now) else {
            return;
        };
        if !y.is_finite() {
            return;
        }
        if self.frames.len() == MAX_FRAMES {
            self.omitted = self.omitted.saturating_add(1);
            return;
        }
        self.frames.push(Frame {
            document,
            offset,
            y,
            initial,
        });
    }

    pub(super) fn to_json(&self) -> String {
        let frames = self.frames.iter().map(|frame| format!(
            "{{\"document\":{},\"offset_ms\":{:.3},\"viewport_y_css_px\":{:.3},\"initial\":{}}}",
            frame.document.get(), frame.offset.as_secs_f64() * 1000.0, frame.y, frame.initial
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"timing_scope\":\"observed native animation position commits, not display scanout\",\"frame_limit\":{MAX_FRAMES},\"omitted_frames\":{},\"frames\":[{frames}]}}",
            self.omitted
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_frames_require_observation_and_keep_explicit_bounds() {
        let now = Instant::now();
        let document = DocumentId::new(1).unwrap();
        let mut trace = AnimationTrace::default();
        trace.record(document, 100.0, true, now);
        assert!(trace.frames.is_empty());
        trace.begin(now);
        trace.begin(now + Duration::from_secs(1));
        trace.record(document, f64::NAN, true, now);
        assert!(trace.frames.is_empty());
        for index in 0..MAX_FRAMES + 2 {
            trace.record(
                document,
                index as f64,
                index == 0,
                now + Duration::from_millis(index as u64),
            );
        }
        let json: serde_json::Value = serde_json::from_str(&trace.to_json()).unwrap();
        assert_eq!(json["omitted_frames"], 2);
        assert_eq!(json["frames"].as_array().unwrap().len(), MAX_FRAMES);
        assert_eq!(json["frames"][0]["initial"], true);
        assert_eq!(json["frames"][1]["initial"], false);
        assert_eq!(json["frames"][1]["offset_ms"], 1.0);
        assert_eq!(json["frames"][1]["viewport_y_css_px"], 1.0);
    }
}
