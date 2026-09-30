//! Bound decoded-image deltas by the remaining encoded presentation capacity.

use super::DocumentRuntime;
use crate::engine::dom::NodeId;
use crate::limits::{
    MAX_PRESENTED_IMAGES, MAX_RENDERER_PRESENTATION_BYTES, MAX_RUNTIME_REPORT_ENTRIES,
};
use crate::renderer_protocol::{PresentedImage, RendererPresentation};

#[cfg(test)]
mod wheel_budget;

pub(super) struct ImageDelta {
    pub presented: PresentedImage,
    pub canvas_update: bool,
    pub frame: Option<NodeId>,
    pub already_sent: bool,
}

impl ImageDelta {
    fn wire_bytes(&self) -> usize {
        // presentation/codec.rs writes a length-prefixed URL, width, height, and
        // length-prefixed BGRA bytes. The empty-images preflight already includes
        // the image count and every other presentation field.
        16 + self.presented.url.len() + self.presented.image.bgra.len()
    }
}

#[derive(Default)]
pub(super) struct ImageSelection {
    pub indexes: Vec<usize>,
    pub deferred: bool,
    pub unpresentable: bool,
}

pub(super) fn select(candidates: &[ImageDelta], available_bytes: usize) -> ImageSelection {
    let mut selection = ImageSelection::default();
    let mut remaining = available_bytes;
    for (index, candidate) in candidates.iter().enumerate() {
        let bytes = candidate.wire_bytes();
        if bytes > available_bytes {
            selection.unpresentable = true;
        } else if selection.indexes.len() == MAX_PRESENTED_IMAGES || bytes > remaining {
            selection.deferred = true;
        } else {
            selection.indexes.push(index);
            remaining -= bytes;
        }
    }
    selection
}

pub(super) fn order_for_delivery(candidates: &mut [ImageDelta], last_served: Option<&str>) {
    // First publication must not starve behind a continuously repainted Canvas.
    // URLs break ties independently of HashMap iteration order.
    candidates.sort_by(|left, right| {
        left.already_sent
            .cmp(&right.already_sent)
            .then_with(|| left.presented.url.cmp(&right.presented.url))
    });
    if let Some(last_served) = last_served {
        let first_update = candidates.partition_point(|candidate| !candidate.already_sent);
        let updates = &mut candidates[first_update..];
        let next =
            updates.partition_point(|candidate| candidate.presented.url.as_str() <= last_served);
        updates.rotate_left(next);
    }
}

impl DocumentRuntime {
    pub(super) fn append_bounded_images(
        &mut self,
        presentation: &mut RendererPresentation,
        candidates: &[ImageDelta],
    ) -> Result<(), String> {
        if candidates.is_empty() {
            return Ok(());
        }
        let base_bytes = presentation
            .encode()
            .map_err(|error| format!("presentation metadata exceeds wire budget: {error}"))?
            .len();
        let mut selection = select(
            candidates,
            MAX_RENDERER_PRESENTATION_BYTES.saturating_sub(base_bytes),
        );
        const WARNING: &str = "A decoded image cannot fit in the renderer presentation wire budget; it may be retried at a later rendering checkpoint";
        if selection.unpresentable
            && selection.indexes.is_empty()
            && presentation.glyphs.is_empty()
            && !self.image_budget_warning_sent
            && presentation.runtime.diagnostics.len() < MAX_RUNTIME_REPORT_ENTRIES
            && base_bytes <= MAX_RENDERER_PRESENTATION_BYTES.saturating_sub(WARNING.len() + 4)
        {
            presentation.runtime.diagnostics.push(WARNING.into());
            self.image_budget_warning_sent = true;
            let base_bytes = presentation
                .encode()
                .map_err(|error| format!("presentation metadata exceeds wire budget: {error}"))?
                .len();
            selection = select(
                candidates,
                MAX_RENDERER_PRESENTATION_BYTES.saturating_sub(base_bytes),
            );
        }
        let mut root_canvas_updates = Vec::new();
        let mut frame_canvas_updates = Vec::new();
        for index in selection.indexes {
            let candidate = &candidates[index];
            let key = &candidate.presented.url;
            self.sent_images.insert(key.clone());
            if candidate.already_sent {
                self.last_served_image_key = Some(key.clone());
            }
            presentation.images.push(candidate.presented.clone());
            if candidate.canvas_update {
                if let Some(frame) = candidate.frame {
                    frame_canvas_updates.push((frame, key.clone()));
                } else {
                    root_canvas_updates.push(key.clone());
                }
            }
        }
        self.page
            .acknowledge_canvas_image_updates(&root_canvas_updates);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.acknowledge_frame_canvas_updates(&frame_canvas_updates);
        }
        // Sent deltas disappear from the next candidate set, so one immediate
        // checkpoint can deliver the remainder. An image that cannot fit even
        // alone is not spun at zero delay after pending glyphs have drained.
        if selection.deferred
            || (selection.unpresentable
                && (!presentation.images.is_empty() || !presentation.glyphs.is_empty()))
        {
            self.rendering.dirty = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DecodedImage;

    fn candidate(name: &str, bytes: usize) -> ImageDelta {
        ImageDelta {
            presented: PresentedImage {
                url: name.into(),
                image: DecodedImage {
                    width: 1,
                    height: 1,
                    bgra: vec![0; bytes].into(),
                },
            },
            canvas_update: false,
            frame: None,
            already_sent: false,
        }
    }

    #[test]
    fn image_deltas_defer_without_exceeding_the_wire_budget() {
        let candidates = [candidate("one", 16), candidate("two", 16)];
        assert_eq!(candidates[0].wire_bytes(), 35);
        let first = select(&candidates, 60);
        assert_eq!(first.indexes, [0]);
        assert!(first.deferred);
        assert!(!first.unpresentable);
        let second = select(&candidates[1..], 60);
        assert_eq!(second.indexes, [0]);
        assert!(!second.deferred);
    }

    #[test]
    fn image_that_cannot_fit_alone_does_not_cause_a_retry_loop() {
        let candidates = [candidate("large", 16)];
        let selection = select(&candidates, 34);
        assert!(selection.indexes.is_empty());
        assert!(!selection.deferred);
        assert!(selection.unpresentable);
    }

    #[test]
    fn never_sent_image_precedes_a_repeated_canvas_update() {
        let mut repainted = candidate("root-canvas", 48);
        repainted.already_sent = true;
        let mut candidates = [repainted, candidate("child-canvas", 32)];
        order_for_delivery(&mut candidates, None);
        let selection = select(&candidates, 100);
        assert_eq!(selection.indexes, [0]);
        assert_eq!(candidates[0].presented.url, "child-canvas");
        assert!(selection.deferred);
    }

    #[test]
    fn repeatedly_dirty_images_rotate_after_the_last_served_key() {
        let mut candidates = [candidate("a", 16), candidate("b", 16), candidate("c", 16)];
        for candidate in &mut candidates {
            candidate.already_sent = true;
        }
        order_for_delivery(&mut candidates, None);
        let first = select(&candidates, 66);
        assert_eq!(first.indexes, [0, 1]);
        let last_served = candidates[*first.indexes.last().unwrap()]
            .presented
            .url
            .clone();
        order_for_delivery(&mut candidates, Some(&last_served));
        assert_eq!(candidates[0].presented.url, "c");
        let second = select(&candidates, 66);
        assert_eq!(second.indexes, [0, 1]);
    }
}
