use super::Page;
use crate::engine::DecodedImage;
use crate::engine::dom::{Dom, NodeId, NodeRef};
use crate::limits::{MAX_PAGE_DECODED_IMAGE_BYTES, MAX_PRESENTED_IMAGES};
use std::collections::HashMap;

const MEDIA_FRAME_KEY_PREFIX: &str = "breeze-internal:media-frame:";
pub(crate) const MEDIA_VIDEO_PLACEHOLDER: &str = "breeze-internal:media-placeholder";

pub(super) fn install_placeholder(dom: &Dom, images: &mut HashMap<String, DecodedImage>) {
    if dom.elements_named("video").next().is_some() {
        let _ = install_initial_decoded_image(
            images,
            MEDIA_VIDEO_PLACEHOLDER.into(),
            DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![0, 0, 0, 255],
            },
        );
    }
}

pub(super) fn install_initial_decoded_image(
    images: &mut HashMap<String, DecodedImage>,
    key: String,
    image: DecodedImage,
) -> Result<(), String> {
    if !images.contains_key(&key) && images.len() >= MAX_PRESENTED_IMAGES {
        return Err(format!(
            "decoded image count exceeds the {MAX_PRESENTED_IMAGES}-image document limit"
        ));
    }
    let retained_bytes = images
        .iter()
        .filter(|(existing, _)| *existing != &key)
        .try_fold(0_usize, |total, (_, image)| {
            total.checked_add(image.bgra.len())
        })
        .ok_or_else(|| "decoded image byte count overflow".to_string())?;
    if !fits_decoded_image_budget(retained_bytes, image.bgra.len()) {
        return Err(format!(
            "decoded images exceed the {MAX_PAGE_DECODED_IMAGE_BYTES}-byte document limit"
        ));
    }
    images.insert(key, image);
    Ok(())
}

fn fits_decoded_image_budget(retained_bytes: usize, incoming_bytes: usize) -> bool {
    retained_bytes
        .checked_add(incoming_bytes)
        .is_some_and(|total| total <= MAX_PAGE_DECODED_IMAGE_BYTES)
}

pub(super) fn image_url(page: &Page, node: &NodeRef) -> Option<String> {
    (node.tag_name() == Some("video")).then(|| {
        let frame = frame_key(node.id());
        if page.images.contains_key(&frame) {
            frame
        } else {
            MEDIA_VIDEO_PLACEHOLDER.into()
        }
    })
}

impl Page {
    pub(super) fn install_decoded_image(
        &mut self,
        url: String,
        image: DecodedImage,
    ) -> Result<(), String> {
        install_initial_decoded_image(&mut self.images, url, image)
    }

    /// Replaces the current decoded frame for one video element after validating engine limits.
    pub fn install_media_frame(
        &mut self,
        node: NodeId,
        image: DecodedImage,
    ) -> Result<String, String> {
        let pixels = u64::from(image.width)
            .checked_mul(u64::from(image.height))
            .ok_or_else(|| "media frame pixel count overflow".to_string())?;
        let expected = pixels
            .checked_mul(4)
            .ok_or_else(|| "media frame byte count overflow".to_string())?;
        if image.width == 0
            || image.height == 0
            || image.width > crate::limits::MAX_MEDIA_DIMENSION
            || image.height > crate::limits::MAX_MEDIA_DIMENSION
            || expected > crate::limits::MAX_MEDIA_DECODED_FRAME_BYTES as u64
            || image.bgra.len() as u64 != expected
        {
            return Err("invalid decoded media frame".into());
        }
        let key = frame_key(node);
        let existing = self
            .images
            .keys()
            .filter(|key| key.starts_with(MEDIA_FRAME_KEY_PREFIX))
            .count();
        if !self.images.contains_key(&key) && existing >= crate::limits::MAX_MEDIA_SESSIONS_PER_TAB
        {
            return Err("media frame session budget exhausted".into());
        }
        self.install_decoded_image(key.clone(), image)?;
        Ok(key)
    }
}

fn frame_key(node: NodeId) -> String {
    format!("{MEDIA_FRAME_KEY_PREFIX}{:032x}", node.to_wire())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: usize) -> DecodedImage {
        DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![0; bytes],
        }
    }

    #[test]
    fn decoded_image_admission_counts_replacements_against_the_aggregate_budget() {
        let mut images = HashMap::from([("icon".to_string(), image(4))]);
        install_initial_decoded_image(&mut images, "icon".into(), image(8)).unwrap();
        assert_eq!(images["icon"].bgra.len(), 8);

        assert!(fits_decoded_image_budget(
            MAX_PAGE_DECODED_IMAGE_BYTES - 8,
            8
        ));
        assert!(!fits_decoded_image_budget(
            MAX_PAGE_DECODED_IMAGE_BYTES - 8,
            9
        ));
        assert!(!fits_decoded_image_budget(usize::MAX, 1));
    }
}
