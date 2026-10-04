//! Current video pixels are installed before media events can run author script.
use super::*;
use crate::engine::DecodedImage;

impl ScriptRuntime {
    pub(crate) fn set_media_image(
        &mut self,
        node: NodeId,
        image: DecodedImage,
        origin_clean: bool,
    ) {
        let mut host = self.host.borrow_mut();
        if let Err(error) = host.media_images.replace(node, image, origin_clean) {
            // The Canvas/WebGL snapshot budget must not break ordinary playback,
            // and a rejected new frame must not leave stale author-readable pixels.
            host.media_images.remove(node);
            host.diagnose(error);
        }
    }

    pub(crate) fn clear_media_image(&mut self, node: NodeId) {
        self.host.borrow_mut().media_images.remove(node);
    }

    pub(crate) fn set_capture_media_images(&mut self, request_id: u64, image: &DecodedImage) {
        let nodes = self.capture_video_nodes(request_id);
        let black = nodes.iter().any(|(_, enabled)| !enabled).then(|| {
            let mut pixels = vec![0; image.bgra.len()];
            for alpha in pixels[3..].iter_mut().step_by(4) {
                *alpha = 255;
            }
            DecodedImage {
                width: image.width,
                height: image.height,
                bgra: pixels.into(),
            }
        });
        for (node, enabled) in nodes {
            self.set_media_image(
                node,
                if enabled {
                    image.clone()
                } else {
                    black.as_ref().unwrap().clone()
                },
                true,
            );
        }
    }
}
