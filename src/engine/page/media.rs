use super::Page;
use crate::engine::DecodedImage;
use crate::engine::dom::{Dom, NodeId, NodeRef};
use std::collections::HashMap;

const MEDIA_FRAME_KEY_PREFIX: &str = "breeze-internal:media-frame:";
pub(crate) const MEDIA_VIDEO_PLACEHOLDER: &str = "breeze-internal:media-placeholder";

pub(super) fn install_placeholder(dom: &Dom, images: &mut HashMap<String, DecodedImage>) {
    if dom.elements_named("video").next().is_some() {
        images.insert(
            MEDIA_VIDEO_PLACEHOLDER.into(),
            DecodedImage {
                width: 1,
                height: 1,
                bgra: vec![0, 0, 0, 255],
            },
        );
    }
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
        self.images.insert(key.clone(), image);
        Ok(key)
    }
}

fn frame_key(node: NodeId) -> String {
    format!("{MEDIA_FRAME_KEY_PREFIX}{:032x}", node.to_wire())
}
