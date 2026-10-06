//! Keep child-realm Canvas pixels in the same bounded image cache as frame resources.

use super::*;
use crate::engine::Page;
use crate::limits::MAX_PAGE_DECODED_IMAGE_BYTES;

impl ChildRuntimes {
    pub(super) fn sync_canvas(&mut self, document: NodeId) {
        let Some(child) = self.children.get_mut(&document) else {
            return;
        };
        let snapshots = match child.take_canvas_presentation() {
            Ok(snapshots) => snapshots,
            Err(error) => {
                child.host.borrow_mut().diagnose(error);
                return;
            }
        };
        let existing = self.images.get(&document).map(|state| &state.decoded);
        if snapshots.is_empty()
            && !existing
                .is_some_and(|images| images.keys().any(|key| Page::is_canvas_image_key(key)))
        {
            return;
        }
        let host = child.host.borrow();
        let mut page = Page::from_frame_document(
            host.document.clone(),
            &host.script_base_url(),
            host.stylesheet_sources.clone(),
            host.quirks_mode,
            host.media_environment,
        );
        drop(host);
        if let Some(existing) = existing {
            page.images.extend(existing.clone());
        }
        page.prune_detached_canvas_images();
        let before_updates = page.images.clone();
        for snapshot in snapshots {
            if let Err(error) = page.install_canvas_bitmap(
                snapshot.node,
                snapshot.width,
                snapshot.height,
                snapshot.content_size,
                snapshot.pixels,
            ) {
                child.host.borrow_mut().diagnose(error);
            }
        }
        let updates = page.take_image_updates();
        let other_bytes = self
            .images
            .iter()
            .filter(|(id, _)| **id != document)
            .flat_map(|(_, state)| state.decoded.values())
            .fold(0_usize, |total, image| {
                total.saturating_add(image.bgra.len())
            });
        let frame_bytes = page.images.values().fold(0_usize, |total, image| {
            total.saturating_add(image.bgra.len())
        });
        let (images, updates) =
            if other_bytes.saturating_add(frame_bytes) > MAX_PAGE_DECODED_IMAGE_BYTES {
                child
                    .host
                    .borrow_mut()
                    .diagnose("child Canvas image budget exhausted".into());
                (before_updates, HashSet::new())
            } else {
                (page.images, updates)
            };
        let state = self.images.entry(document).or_default();
        state.decoded = images;
        state
            .image_updates
            .retain(|key| state.decoded.contains_key(key));
        state.image_updates.extend(updates);
    }
}

impl ScriptRuntime {
    /// A frame paint snapshot can be discarded (for example by a zero-sized
    /// iframe). Clear pending updates only after those bytes enter a presentation.
    pub(crate) fn acknowledge_frame_image_updates(&mut self, emitted: &[(NodeId, String)]) {
        let Some(frames) = self.frames.as_mut() else {
            return;
        };
        for (document, key) in emitted {
            if let Some(state) = frames.images.get_mut(document) {
                state.image_updates.remove(key);
            }
        }
    }
}
