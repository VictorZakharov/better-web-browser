//! Arrived images presented into the last frame's display list while the next frame waits.
//
// A checkpoint that skips its frame (see `frame_waits`) still presents an arrived image whose box
// from the last layout already fits it: its natural size never entered layout (see
// `Page::layout_read_image_size`), or the box was reserved with the image's own proportions
// from its `width` and `height` attributes (see `replaced_box_follows_ratio`), so the owed frame
// later corrects it by under 1%. Chromium likewise paints an arrived image whose size is already
// known without waiting for layout. Arrived images whose boxes do not fit wait for that frame.
use super::*;
use crate::engine::css::StyleSet;
use crate::engine::dom::Node;
use crate::engine::{DecodedImage, DisplayItem, Page};

impl DocumentRuntime {
    /// The arrived images to withhold when presenting the others without the style and layout
    /// of a full frame, or `None` when no arrived image fits the box it was laid out with.
    pub(in crate::renderer_process::child::document) fn paint_only_image_arrivals(
        &self,
    ) -> Option<HashSet<String>> {
        let arrived = PendingImages::new(
            self.page
                .images
                .iter()
                .filter(|(url, _)| !self.sent_images.contains(*url))
                .map(|(url, image)| (url.as_str(), image)),
        )?;
        // Arrived images the display list shows, and whether every box keeps their proportions.
        let mut fits = HashMap::<&str, bool>::new();
        for item in &self.layout.items {
            let DisplayItem::Image { rect, url, .. } = item else {
                continue;
            };
            let Some((url, image)) = arrived.get(url) else {
                continue;
            };
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
        boxes_following_image_ratios(&self.page, styles, urls)
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

/// Arrived images not yet presented. A checkpoint asks this of every image item in the display
/// list, which a long article has thousands of, while usually only a few images are new; hashing
/// each item's URL twice made this scan a measurable share of a page load's later checkpoints.
enum PendingImages<'a> {
    Few(Vec<(&'a str, &'a DecodedImage)>),
    Many(HashMap<&'a str, &'a DecodedImage>),
}

impl<'a> PendingImages<'a> {
    /// Up to this many, comparing URLs (most differ in length) beats hashing them.
    const FEW: usize = 16;

    fn new(images: impl Iterator<Item = (&'a str, &'a DecodedImage)>) -> Option<Self> {
        let images = images.collect::<Vec<_>>();
        match images.len() {
            0 => None,
            count if count <= Self::FEW => Some(Self::Few(images)),
            _ => Some(Self::Many(images.into_iter().collect())),
        }
    }

    fn get(&self, url: &str) -> Option<(&'a str, &'a DecodedImage)> {
        match self {
            Self::Few(images) => images.iter().copied().find(|(pending, _)| *pending == url),
            Self::Many(images) => images.get_key_value(url).map(|(url, image)| (*url, *image)),
        }
    }
}

/// Those of `urls` that only images show, each with a box that follows its ratio. `styles` are
/// the last frame's and the document may have changed since: this checkpoint skipped its style
/// refresh. An element that has no style yet has no box either, so what it shows waits for the
/// frame that lays it out.
fn boxes_following_image_ratios<'a>(
    page: &Page,
    styles: &StyleSet,
    urls: &[&'a str],
) -> HashSet<&'a str> {
    let mut follows = HashMap::<&str, bool>::new();
    for node in Node::descendants(&page.dom.document) {
        if !matches!(
            node.tag_name(),
            Some("img" | "input" | "video" | "object" | "embed")
        ) {
            continue;
        }
        let Some(url) = page.image_url(&node) else {
            continue;
        };
        let Some(&url) = urls.iter().find(|candidate| **candidate == url) else {
            continue;
        };
        let image = &page.images[url];
        let fits = node.tag_name() == Some("img")
            && styles.style_if_computed(&node).is_some_and(|style| {
                crate::engine::layout::replaced_box_follows_ratio(
                    style,
                    (image.width, image.height),
                )
            });
        *follows.entry(url).or_insert(true) &= fits;
    }
    follows
        .into_iter()
        .filter_map(|(url, fits)| fits.then_some(url))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DecodedImage;

    const URL: &str = "https://example.test/a.png";

    fn page_with_image() -> Page {
        let mut page = Page::parse_scripted(
            r#"<style>img{height:auto}</style><img src="a.png" width="20" height="10">"#,
            "https://example.test/",
        );
        page.images.insert(
            URL.into(),
            DecodedImage {
                width: 2,
                height: 1,
                bgra: vec![0; 8].into(),
            },
        );
        page.refresh_resources_for_viewport(800.0, 600.0);
        page
    }

    #[test]
    fn pending_images_find_each_arrival_whether_few_or_many() {
        let image = DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![0; 4].into(),
        };
        assert!(PendingImages::new(std::iter::empty()).is_none());
        for count in [1, PendingImages::FEW, PendingImages::FEW + 1, 200] {
            let urls = (0..count)
                .map(|index| format!("https://example.test/{index}.png"))
                .collect::<Vec<_>>();
            let pending =
                PendingImages::new(urls.iter().map(|url| (url.as_str(), &image))).unwrap();
            assert_eq!(
                matches!(pending, PendingImages::Few(_)),
                count <= PendingImages::FEW
            );
            for url in &urls {
                assert_eq!(pending.get(url).map(|(found, _)| found), Some(url.as_str()));
            }
            assert!(pending.get("https://example.test/other.png").is_none());
        }
    }

    #[test]
    fn a_reserved_box_follows_its_image_ratio() {
        let page = page_with_image();
        let styles = page.cached_style_for_viewport(800.0, 600.0).unwrap();
        assert!(boxes_following_image_ratios(&page, styles, &[URL]).contains(URL));
    }

    /// The renderer panicked here when an image arrived between a parser or script insertion of
    /// another element showing it and the frame that would style that element.
    #[test]
    fn an_element_inserted_after_the_last_style_refresh_waits_for_its_frame() {
        let page = page_with_image();
        let added = Node::create_element_for(&page.dom.document, "img");
        added.set_attr("src", "a.png");
        let html = page.dom.elements_named("html").next().unwrap();
        Node::append_child(&html, added);
        let styles = page.cached_style_for_viewport(800.0, 600.0).unwrap();
        assert!(boxes_following_image_ratios(&page, styles, &[URL]).is_empty());
    }
}
