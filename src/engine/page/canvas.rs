//! Renderer-owned Canvas bitmaps share the bounded decoded-image presentation path.

use super::*;
use crate::limits::MAX_CANVAS_PIXELS;

const KEY_PREFIX: &str = "breeze-internal:canvas:";

fn key(node: NodeId, content_size: (u32, u32)) -> String {
    format!(
        "{KEY_PREFIX}{}:{}x{}",
        node.to_wire(),
        content_size.0,
        content_size.1
    )
}

/// HTML canvas width and height content attributes are non-negative integers.
/// https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element
pub(crate) fn intrinsic_size(node: &NodeRef) -> (u32, u32) {
    fn dimension(node: &NodeRef, name: &str, fallback: u32) -> u32 {
        let Some(raw) = node.attr(name) else {
            return fallback;
        };
        let digits = raw.trim().as_bytes();
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return fallback;
        }
        digits.iter().fold(0_u32, |value, digit| {
            value
                .saturating_mul(10)
                .saturating_add(u32::from(*digit - b'0'))
        })
    }
    (
        dimension(node, "width", 300),
        dimension(node, "height", 150),
    )
}

pub(super) fn image_url(page: &Page, node: &NodeRef) -> Option<String> {
    if !page.scripting_enabled {
        return None;
    }
    let key = key(node.id(), intrinsic_size(node));
    page.images.contains_key(&key).then_some(key)
}

impl Page {
    pub(crate) fn is_canvas_image_key(key: &str) -> bool {
        key.starts_with(KEY_PREFIX)
    }

    pub(crate) fn install_canvas_bitmap(
        &mut self,
        node_id: NodeId,
        width: u32,
        height: u32,
        content_size: (u32, u32),
        pixels: Option<Vec<u8>>,
    ) -> Result<(), String> {
        let Some(node) = self.dom.find_node(node_id) else {
            return Ok(());
        };
        if node.tag_name() != Some("canvas")
            || Node::shadow_including_root(&node).id() != self.dom.document.id()
            || intrinsic_size(&node) != content_size
        {
            return Ok(());
        }
        let key = key(node_id, content_size);
        let count = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| "Canvas bitmap dimensions overflow".to_string())?;
        let Some(mut pixels) = pixels else {
            self.images.remove(&key);
            self.canvas_image_updates.remove(&key);
            return Ok(());
        };
        if count == 0 || count > MAX_CANVAS_PIXELS || pixels.len() != count * 4 {
            self.images.remove(&key);
            self.canvas_image_updates.remove(&key);
            return Err("Canvas bitmap exceeds its pixel limit or has invalid dimensions".into());
        }
        // Canvas owns straight-alpha RGBA; GDI presents premultiplied BGRA.
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
            pixel.swap(0, 2);
        }
        let result = media::install_initial_decoded_image(
            &mut self.images,
            key.clone(),
            DecodedImage {
                width,
                height,
                bgra: pixels.into(),
            },
        );
        if let Err(error) = result {
            self.images.remove(&key);
            self.canvas_image_updates.remove(&key);
            return Err(error);
        }
        self.canvas_image_updates.insert(key);
        Ok(())
    }

    pub(crate) fn take_canvas_image_updates(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.canvas_image_updates)
    }

    pub(crate) fn has_canvas_image_update(&self, key: &str) -> bool {
        self.canvas_image_updates.contains(key)
    }

    pub(crate) fn acknowledge_canvas_image_updates(&mut self, keys: &[String]) {
        for key in keys {
            self.canvas_image_updates.remove(key);
        }
    }

    pub(crate) fn prune_detached_canvas_images(&mut self) {
        self.images.retain(|key, _| {
            let Some(encoded) = key.strip_prefix(KEY_PREFIX) else {
                return true;
            };
            let connected = encoded
                .split(':')
                .next()
                .unwrap_or_default()
                .parse::<u128>()
                .ok()
                .and_then(NodeId::from_wire)
                .and_then(|id| self.dom.find_node(id))
                .is_some_and(|node| {
                    Node::shadow_including_root(&node).id() == self.dom.document.id()
                        && key == &self::key(node.id(), intrinsic_size(&node))
                });
            if !connected {
                self.canvas_image_updates.remove(key);
            }
            connected
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_pixels_are_presented_in_premultiplied_bgra_and_reset_on_resize() {
        let mut page = Page::parse_scripted(
            "<canvas width=2 height=1></canvas>",
            "https://example.test/",
        );
        let node = page.dom.elements_named("canvas").next().unwrap();
        page.install_canvas_bitmap(
            node.id(),
            2,
            1,
            (2, 1),
            Some(vec![200, 100, 50, 128, 0, 0, 0, 0]),
        )
        .unwrap();
        let key = image_url(&page, &node).unwrap();
        assert_eq!(&*page.images[&key].bgra, &[25, 50, 100, 128, 0, 0, 0, 0]);
        assert!(page.take_canvas_image_updates().contains(&key));
        node.set_attr("width", "3");
        assert_eq!(image_url(&page, &node), None);
        page.install_canvas_bitmap(node.id(), 3, 1, (3, 1), None)
            .unwrap();
        page.prune_detached_canvas_images();
        assert!(!page.images.contains_key(&key));
    }

    #[test]
    fn natural_bitmap_size_is_independent_but_its_content_stamp_rejects_stale_resize() {
        let mut page = Page::parse_scripted("<canvas width=9 height=8></canvas>", "about:blank");
        let node = page.dom.elements_named("canvas").next().unwrap();
        let pixels = [255, 0, 0, 255].repeat(6);
        page.install_canvas_bitmap(node.id(), 3, 2, (9, 8), Some(pixels.clone()))
            .unwrap();
        let url = image_url(&page, &node).unwrap();
        assert_eq!((page.images[&url].width, page.images[&url].height), (3, 2));
        node.set_attr("width", "10");
        assert!(image_url(&page, &node).is_none());
        page.prune_detached_canvas_images();
        assert!(!page.images.contains_key(&url));
        page.install_canvas_bitmap(node.id(), 3, 2, (9, 8), Some(pixels))
            .unwrap();
        assert!(
            page.images.is_empty(),
            "stale snapshots must not reinstall resized content"
        );
    }
}
