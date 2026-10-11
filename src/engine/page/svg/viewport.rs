//! Inline SVG's CSS viewport and its device raster are different quantities.
//! SVG 2 §8.3/§8.12: the containing CSS box establishes the initial viewport;
//! neither that used size nor device density changes the element's intrinsic size.
//! https://www.w3.org/TR/SVG2/coords.html#SVGViewport
use super::*;
use crate::engine::dom::NodeId;
use crate::engine::layout::{DisplayItem, LayoutOutput};
use std::collections::HashMap;
use std::sync::Arc;
mod raster;
use raster::Request;

const MAX_RETAINED_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Default)]
pub(in crate::engine::page) struct State {
    pub versions: HashMap<NodeId, u64>,
    pub enabled: bool,
    entries: HashMap<String, Entry>,
    intrinsic: HashMap<String, (u32, u32)>,
}

#[derive(Debug)]
struct Entry {
    node: NodeId,
    source: Arc<str>,
    fonts: Arc<[crate::engine::font::WebFont]>,
    published: Option<Request>,
}

impl State {
    pub fn snapshot(&self) -> Self {
        Self {
            intrinsic: self.intrinsic.clone(),
            ..Self::default()
        }
    }

    pub fn synchronize(&mut self, source: &Self) {
        self.intrinsic.clone_from(&source.intrinsic);
    }

    pub fn retain(&mut self, ids: &HashSet<NodeId>, keys: &HashSet<String>) {
        self.versions.retain(|id, _| ids.contains(id));
        self.entries.retain(|key, _| keys.contains(key));
        self.intrinsic.retain(|key, _| keys.contains(key));
    }

    pub fn forget(&mut self, key: &str) {
        self.entries.remove(key);
        self.intrinsic.remove(key);
    }

    pub fn can_admit(&self, key: &str, input: &InlineSvgInput) -> bool {
        if !self.enabled {
            return false;
        }
        let incoming = input.source.as_ref().map_or(0, |source| source.len());
        self.entries
            .iter()
            .filter(|(existing, _)| existing.as_str() != key)
            .try_fold(incoming, |total, (_, entry)| {
                total.checked_add(entry.source.len())
            })
            .is_some_and(|total| total <= MAX_RETAINED_SOURCE_BYTES)
    }

    pub fn prepare(
        &mut self,
        node: NodeId,
        key: String,
        input: &InlineSvgInput,
    ) -> Result<(), String> {
        let source = input.source.as_ref().map_err(Clone::clone)?;
        // Parse once for the existing intrinsic-size contract, without allocating
        // or filtering natural-size pixels. The used viewport is known after layout.
        let tree = raster::tree(source, &input.fonts, None)?;
        let size = tree.size().to_int_size();
        let natural = (size.width(), size.height());
        self.intrinsic.insert(key.clone(), natural);
        self.entries.insert(
            key,
            Entry {
                node,
                source: Arc::from(source.as_str()),
                fonts: input.fonts.clone(),
                published: None,
            },
        );
        Ok(())
    }
}

impl Page {
    pub(crate) fn enable_inline_svg_viewports(&mut self) {
        self.inline_svgs.enabled = true;
    }

    /// Layout/CSSOM need natural dimensions, never the last HiDPI pixel extent.
    pub(crate) fn layout_image_dimensions(&self, key: &str) -> Option<(u32, u32)> {
        self.inline_svgs.intrinsic.get(key).copied().or_else(|| {
            self.images
                .get(key)
                .map(|image| (image.width, image.height))
        })
    }

    pub(crate) fn prepare_inline_svg_images(&mut self, layout: &LayoutOutput) {
        if !self.inline_svgs.enabled {
            return;
        }
        let density = self.media_environment.resolution_dppx;
        let mut requests = Requests::default();
        requests.collect(&layout.items, density);
        for (key, requested) in requests.ordered {
            let Some(entry) = self.inline_svgs.entries.get(&key) else {
                continue;
            };
            if let Ok(request) = &requested
                && entry.published == Some(*request)
                && self.images.contains_key(&key)
            {
                continue;
            }
            let node = entry.node;
            let result = requested
                .and_then(|request| {
                    let image = raster::render(&entry.source, &entry.fonts, request)?;
                    Ok((request, image))
                })
                .and_then(|(request, image)| {
                    self.install_decoded_image(key.clone(), image)?;
                    Ok(request)
                });
            match result {
                Ok(request) => {
                    self.inline_svgs.entries.get_mut(&key).unwrap().published = Some(request);
                    self.image_updates.insert(key);
                }
                Err(error) => {
                    self.images.remove(&key);
                    self.image_updates.remove(&key);
                    self.report_inline_svg_error(node, &error);
                }
            }
        }
    }
}

#[derive(Default)]
struct Requests {
    ordered: Vec<(String, Result<Request, String>)>,
    indices: HashMap<String, usize>,
}
impl Requests {
    fn collect(&mut self, items: &[DisplayItem], density: f32) {
        for item in items {
            match item {
                DisplayItem::Image { rect, url, .. }
                    if url.starts_with("inline-svg:") && rect.width > 0. && rect.height > 0. =>
                {
                    let request = Request::new(rect.width, rect.height, density);
                    if let Some(&index) = self.indices.get(url) {
                        self.ordered[index].1 = request;
                    } else if self.ordered.len() < MAX_INLINE_SVGS {
                        self.indices.insert(url.clone(), self.ordered.len());
                        self.ordered.push((url.clone(), request));
                    }
                }
                DisplayItem::BeginCssMask { layers, .. } => self.collect(layers, density),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
