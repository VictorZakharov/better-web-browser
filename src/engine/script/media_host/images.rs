//! Renderer-owned current video frames. Author objects never supply native pixels or origin policy.
use super::super::*;
use crate::engine::DecodedImage;

const MAX_VIDEO_SOURCES: usize = 8;
const MAX_VIDEO_BYTES: usize = 32 * 1024 * 1024;

pub(in crate::engine::script) struct VideoImage {
    image: DecodedImage,
    origin_clean: bool,
}

#[derive(Default)]
pub(in crate::engine::script) struct MediaImages {
    frames: HashMap<NodeId, VideoImage>,
    bytes: usize,
}

impl MediaImages {
    pub(in crate::engine::script) fn replace(
        &mut self,
        node: NodeId,
        image: DecodedImage,
        origin_clean: bool,
    ) -> Result<(), String> {
        let expected = (image.width as usize)
            .checked_mul(image.height as usize)
            .filter(|count| *count != 0 && *count <= crate::limits::MAX_CANVAS_PIXELS)
            .and_then(|count| count.checked_mul(4))
            .ok_or("Video image dimensions exceed the snapshot budget")?;
        if expected != image.bgra.len() {
            return Err("Video image bytes do not match their dimensions".into());
        }
        let previous = self
            .frames
            .get(&node)
            .map_or(0, |frame| frame.image.bgra.len());
        let total = self.bytes - previous + expected;
        if total > MAX_VIDEO_BYTES
            || !self.frames.contains_key(&node) && self.frames.len() >= MAX_VIDEO_SOURCES
        {
            return Err("Current video images exceed the document snapshot budget".into());
        }
        // Retain the decoder's immutable Arc, not a second per-frame pixel allocation.
        // Convert to an owned RGBA snapshot only when script actually requests one.
        self.frames.insert(
            node,
            VideoImage {
                image,
                origin_clean,
            },
        );
        self.bytes = total;
        Ok(())
    }

    pub(in crate::engine::script) fn remove(&mut self, node: NodeId) {
        if let Some(frame) = self.frames.remove(&node) {
            self.bytes -= frame.image.bgra.len();
        }
    }

    pub(in crate::engine::script) fn blacken(&mut self, node: NodeId) {
        let Some(frame) = self.frames.get_mut(&node) else {
            return;
        };
        let mut pixels = vec![0; frame.image.bgra.len()];
        for alpha in pixels[3..].iter_mut().step_by(4) {
            *alpha = 255;
        }
        frame.image.bgra = pixels.into();
    }

    fn snapshot(&self, node: NodeId) -> JsValue {
        let Some(frame) = self.frames.get(&node) else {
            return JsValue::Null;
        };
        // Origin cleanliness comes from the final browser fetch response. In particular,
        // changing crossOrigin after loading cannot declassify an opaque frame.
        if !frame.origin_clean {
            return JsValue::from("tainted".to_string());
        }
        let mut rgba = Vec::with_capacity(frame.image.bgra.len());
        for pixel in frame.image.bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        JsValue::Array(vec![
            JsValue::from(f64::from(frame.image.width)),
            JsValue::from(f64::from(frame.image.height)),
            JsValue::Bytes(rgba),
        ])
    }
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> Option<JsValue> {
    if operation != "mediaVideoSnapshot" {
        return None;
    }
    let Some(node) = state
        .node(super::argument_id(args, 1))
        .filter(|node| node.tag_name() == Some("video"))
    else {
        return Some(JsValue::Null);
    };
    Some(state.media_images.snapshot(node.id()))
}

#[cfg(test)]
mod tests;
