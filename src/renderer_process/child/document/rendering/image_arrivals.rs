//! Arrived images presented into the last frame's display list while the next frame waits.
//
// A checkpoint that skips its frame (see `frame_waits`) still presents an arrived image whose box
// from the last layout already fits it: its natural size never entered layout (see
// `Page::layout_read_image_size`), or the box was reserved with the image's own proportions
// from its `width` and `height` attributes (see `replaced_box_follows_ratio`), so the owed frame
// later corrects it by under 1%. Chromium likewise paints an arrived image whose size is already
// known without waiting for layout. Arrived images whose boxes do not fit wait for that frame.
use super::*;
use crate::engine::DisplayItem;
use crate::engine::dom::Node;

impl DocumentRuntime {
    /// The arrived images to withhold when presenting the others without the style and layout
    /// of a full frame, or `None` when no arrived image fits the box it was laid out with.
    pub(in crate::renderer_process::child::document) fn paint_only_image_arrivals(
        &self,
    ) -> Option<HashSet<String>> {
        if self
            .page
            .images
            .keys()
            .all(|url| self.sent_images.contains(url))
        {
            return None;
        }
        // Arrived images the display list shows, and whether every box keeps their proportions.
        let mut fits = HashMap::<&str, bool>::new();
        for item in &self.layout.items {
            let DisplayItem::Image { rect, url, .. } = item else {
                continue;
            };
            let Some(image) = self.page.images.get(url) else {
                continue;
            };
            if self.sent_images.contains(url) {
                continue;
            }
            let proportional = rect.width > 0.0
                && rect.height > 0.0
                && ((rect.width * image.height as f32) / (rect.height * image.width as f32) - 1.0)
                    .abs()
                    <= 0.01;
            *fits.entry(url).or_insert(true) &= proportional;
        }
        let reserved: Vec<&str> = fits
            .iter()
            .filter(|(url, proportional)| **proportional && self.page.layout_read_image_size(url))
            .map(|(url, _)| *url)
            .collect();
        let reserved = self.boxes_follow_image_ratios(&reserved);
        for (url, fit) in &mut fits {
            *fit = !self.page.layout_read_image_size(url) || reserved.contains(url);
        }
        fits.values().any(|fit| *fit).then(|| {
            fits.iter()
                .filter(|(_, fit)| !**fit)
                .map(|(url, _)| url.to_string())
                .collect()
        })
    }

    /// Those of `urls` that only images show, each with a box that follows its ratio.
    fn boxes_follow_image_ratios<'a>(&self, urls: &[&'a str]) -> HashSet<&'a str> {
        let Some(styles) = self
            .page
            .cached_style_for_viewport(self.viewport.style_width, self.viewport.height)
            .filter(|_| !urls.is_empty())
        else {
            return HashSet::new();
        };
        let mut follows = HashMap::<&str, bool>::new();
        for node in Node::descendants(&self.page.dom.document) {
            if !matches!(
                node.tag_name(),
                Some("img" | "input" | "video" | "object" | "embed")
            ) {
                continue;
            }
            let Some(url) = self.page.image_url(&node) else {
                continue;
            };
            let Some(&url) = urls.iter().find(|candidate| **candidate == url) else {
                continue;
            };
            let image = &self.page.images[url];
            let fits = node.tag_name() == Some("img")
                && crate::engine::layout::replaced_box_follows_ratio(
                    &styles.get(&node),
                    (image.width, image.height),
                );
            *follows.entry(url).or_insert(true) &= fits;
        }
        follows
            .into_iter()
            .filter_map(|(url, fits)| fits.then_some(url))
            .collect()
    }

    /// Skips this checkpoint's frame like `delay_background_frame`, but presents the last
    /// frame's display list again with the arrived images that are not `withheld`. The skipped
    /// frame is still owed: its wake-up, the spacing from the last full frame and the deferred
    /// style and layout work all stay.
    pub(in crate::renderer_process::child::document) fn present_arrived_images(
        &mut self,
        outcome: ScriptOutcome,
        resources: [bool; 3],
        load: PageLoadReport,
        withheld: HashSet<String>,
    ) -> Result<AdvanceResult, String> {
        self.defer_background_frame(&outcome, resources);
        let last_frame = self.rendering.last_frame;
        self.withheld_images = withheld;
        let presentation =
            self.presentation_after_observers(outcome, StyleRefreshStats::default(), load, None);
        self.withheld_images.clear();
        let mut presentation = presentation?;
        self.rendering.dirty = true;
        self.rendering.frame_delayed = true;
        self.rendering.last_frame = last_frame;
        match &mut presentation {
            AdvanceResult::Presentation(value) => value.clock_advanced = true,
            AdvanceResult::Runtime(value) => value.clock_advanced = true,
            AdvanceResult::EncodingRestart => {}
        }
        Ok(presentation)
    }
}
