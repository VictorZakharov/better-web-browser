//! Bounded dimensional samples identify slow native draws without exporting URLs or pixels.
use super::*;
#[derive(Clone, Copy, Default)]
pub(super) struct Summary {
    pub(super) ms: f64,
    source: [u32; 2],
    destination: [u32; 2],
    opaque: bool,
    kind: &'static str,
}
impl Summary {
    pub(super) fn to_json(self) -> serde_json::Value {
        serde_json::json!({ "ms": self.ms, "source": self.source, "destination": self.destination, "opaque": self.opaque, "kind": self.kind })
    }
}
pub(in crate::windows_app::painting) struct ImageTiming(Option<(Instant, Summary)>);
impl ImageTiming {
    pub(in crate::windows_app::painting) fn native(
        source: [u32; 2],
        dc: crate::windows_app::platform::Hdc,
        destination: crate::windows_app::platform::Rect,
        opaque: bool,
        kind: &'static str,
    ) -> Self {
        if ACTIVE.get().is_none() {
            return Self(None);
        }
        let bounds =
            unsafe { crate::windows_app::painting::affine::device_bounds(dc, destination) };
        Self::start(
            source,
            [bounds.width().max(0) as u32, bounds.height().max(0) as u32],
            opaque,
            kind,
        )
    }

    pub(in crate::windows_app::painting) fn start(
        source: [u32; 2],
        destination: [u32; 2],
        opaque: bool,
        kind: &'static str,
    ) -> Self {
        Self(ACTIVE.get().map(|_| {
            (
                Instant::now(),
                Summary {
                    source,
                    destination,
                    opaque,
                    kind,
                    ms: 0.0,
                },
            )
        }))
    }
}
impl Drop for ImageTiming {
    fn drop(&mut self) {
        if let Some((started, mut sample)) = self.0
            && let Some(mut totals) = ACTIVE.get()
        {
            sample.ms = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(index) = totals
                .slow_images
                .iter()
                .position(|other| sample.ms > other.ms)
            {
                totals.slow_images[index..].rotate_right(1);
                totals.slow_images[index] = sample;
                ACTIVE.set(Some(totals));
            }
        }
    }
}
