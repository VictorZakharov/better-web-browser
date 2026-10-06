//! Connected image snapshots share decoder Arcs, not author-controlled dimensions or bytes.
use super::super::*;
use crate::engine::DecodedImage;

#[derive(Default)]
pub(in crate::engine::script) struct ElementImages {
    images: HashMap<String, (DecodedImage, bool)>,
}

impl ElementImages {
    pub(in crate::engine::script) fn synchronize(
        &mut self,
        images: &HashMap<String, DecodedImage>,
        origins: &HashMap<String, bool>,
    ) {
        // The Page already bounds total retained image memory. Canvas additionally bounds
        // a single snapshot before BGRA -> RGBA allocates a new script-owned packet.
        // A layout-only snapshot has empty pixels and must not grant readback authority.
        self.images.clear();
        for (url, image) in images {
            let Some(clean) = origins.get(url) else {
                continue;
            };
            let pixels = (image.width as usize).checked_mul(image.height as usize);
            if !pixels.is_some_and(|count| {
                count != 0
                    && count <= crate::limits::MAX_CANVAS_PIXELS
                    && count.checked_mul(4) == Some(image.bgra.len())
            }) {
                continue;
            }
            self.images.insert(url.clone(), (image.clone(), *clean));
        }
    }

    fn snapshot(&self, url: &str) -> JsValue {
        let Some((image, clean)) = self.images.get(url) else {
            return JsValue::Null;
        };
        // Until Canvas has end-to-end tainted bitmap ownership, reject opaque inputs
        // before exposing pixels. This matches the existing video snapshot boundary;
        // it is deliberately not claimed as full tainted Canvas drawing support.
        if !clean {
            return JsValue::from("tainted".to_string());
        }
        let mut rgba = Vec::with_capacity(image.bgra.len());
        for pixel in image.bgra.chunks_exact(4) {
            let alpha = u32::from(pixel[3]);
            let straight = |value: u8| {
                (u32::from(value) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0)
                    .min(255) as u8
            };
            rgba.extend_from_slice(&[
                straight(pixel[2]),
                straight(pixel[1]),
                straight(pixel[0]),
                pixel[3],
            ]);
        }
        JsValue::Array(vec![
            JsValue::from(f64::from(image.width)),
            JsValue::from(f64::from(image.height)),
            JsValue::Bytes(rgba),
        ])
    }
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> Option<JsValue> {
    if !matches!(
        operation,
        "imageElementSource" | "imageElementBitmap" | "imageElementMetadata"
    ) {
        return None;
    }
    let Some(node) = state
        .node(binding_helpers::argument_id(args, 1))
        .filter(|node| {
            node.tag_name() == Some("img")
                && node.namespace_uri() == Some("http://www.w3.org/1999/xhtml")
        })
    else {
        return Some(JsValue::Null);
    };
    let url = crate::engine::Page::selected_image_source(
        &node,
        &state.script_base_url(),
        state.media_environment,
    );
    Some(if operation == "imageElementSource" {
        url.map(JsValue::from).unwrap_or(JsValue::Null)
    } else if operation == "imageElementMetadata" {
        url.as_deref()
            .and_then(|url| {
                state.element_images.images.get(url).map(|(image, _)| {
                    JsValue::Array(vec![
                        JsValue::from(url.to_owned()),
                        JsValue::from(f64::from(image.width)),
                        JsValue::from(f64::from(image.height)),
                    ])
                })
            })
            .unwrap_or(JsValue::Null)
    } else {
        url.as_deref()
            .map(|url| state.element_images.snapshot(url))
            .unwrap_or(JsValue::Null)
    })
}

#[cfg(test)]
mod tests;
