//! Opt-in, UI-thread-local item timings; ordinary painting never starts a clock.
use better_web_browser::engine::DisplayItem;
use std::cell::Cell;
use std::time::Instant;
mod images;
pub(super) use images::ImageTiming;

const NAMES: [&str; 8] = [
    "solid", "border", "text", "image", "control", "group", "clip", "other",
];

#[derive(Clone, Copy, Default)]
pub(in crate::windows_app) struct Totals {
    milliseconds: [f64; 8],
    calls: [usize; 8],
    slow_images: [images::Summary; 8],
}

thread_local! {
    static ACTIVE: Cell<Option<Totals>> = const { Cell::new(None) };
}

pub(in crate::windows_app) struct Capture;

impl Capture {
    pub(in crate::windows_app) fn start() -> Self {
        ACTIVE.set(Some(Totals::default()));
        Self
    }

    pub(in crate::windows_app) fn totals(&self) -> Totals {
        ACTIVE.get().unwrap_or_default()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        ACTIVE.set(None);
    }
}

impl Totals {
    pub(in crate::windows_app) fn to_json(self) -> serde_json::Value {
        let mut categories = NAMES
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    name.into(),
                    serde_json::json!({
                        "calls": self.calls[index], "ms": self.milliseconds[index],
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        categories.insert(
            "slow_images".into(),
            self.slow_images
                .into_iter()
                .filter(|sample| sample.ms > 0.0)
                .map(|sample| sample.to_json())
                .collect::<Vec<_>>()
                .into(),
        );
        categories.into()
    }
}

pub(super) struct ItemTiming(Option<(usize, Instant)>);

impl ItemTiming {
    pub(super) fn start(item: &DisplayItem) -> Self {
        if ACTIVE.get().is_none() {
            return Self(None);
        }
        let index = match item {
            DisplayItem::SolidRect { .. } => 0,
            DisplayItem::BorderRect { .. } => 1,
            DisplayItem::Text { .. } => 2,
            DisplayItem::Image { .. } | DisplayItem::BackgroundImage { .. } => 3,
            DisplayItem::Control(_) => 4,
            DisplayItem::BeginOpacity { .. }
            | DisplayItem::EndOpacity { .. }
            | DisplayItem::BeginMask { .. }
            | DisplayItem::EndMask { .. } => 5,
            DisplayItem::BeginClip { .. } | DisplayItem::EndClip { .. } => 6,
            _ => 7,
        };
        Self(Some((index, Instant::now())))
    }
}

impl Drop for ItemTiming {
    fn drop(&mut self) {
        if let Some((index, started)) = self.0
            && let Some(mut totals) = ACTIVE.get()
        {
            totals.calls[index] += 1;
            totals.milliseconds[index] += started.elapsed().as_secs_f64() * 1000.0;
            ACTIVE.set(Some(totals));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> DisplayItem {
        DisplayItem::EndClip {
            bounds: better_web_browser::engine::RectF::default(),
        }
    }

    #[test]
    fn clocks_are_disabled_outside_a_capture_and_capture_destruction_clears_state() {
        assert!(ItemTiming::start(&item()).0.is_none());
        {
            let capture = Capture::start();
            assert!(ItemTiming::start(&item()).0.is_some());
            assert_eq!(capture.totals().calls[6], 1);
        }
        assert!(ACTIVE.get().is_none());
        assert!(ItemTiming::start(&item()).0.is_none());
    }
    #[test]
    fn each_new_capture_has_independent_counts() {
        {
            let capture = Capture::start();
            drop(ItemTiming::start(&item()));
            assert_eq!(capture.totals().calls[6], 1);
        }
        let capture = Capture::start();
        assert_eq!(capture.totals().calls, [0; 8]);
    }
}
