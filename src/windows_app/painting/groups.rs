//! One stack preserves destination ownership across mixed opacity and mask groups.
use super::super::platform::*;
use super::{
    masking::MaskLayer,
    opacity::{OpacityLayer, OpacityLayerStart},
};
use better_web_browser::engine::{DecodedImage, DisplayItem};
use std::collections::HashMap;

enum Layer {
    Opacity(OpacityLayer),
    Mask(MaskLayer),
    Transform { dc: Hdc, saved: i32 },
}
#[derive(Default)]
pub(super) struct PaintGroups {
    layers: Vec<(Layer, usize)>,
    skipped: usize,
    bytes: usize,
}
impl PaintGroups {
    pub(super) fn dc(&self, root: Hdc) -> Hdc {
        self.layers
            .iter()
            .rev()
            .find_map(|(layer, _)| match layer {
                Layer::Opacity(v) => v.dc(),
                Layer::Mask(v) => Some(v.dc()),
                Layer::Transform { .. } => None,
            })
            .unwrap_or(root)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn handle(
        &mut self,
        item: &DisplayItem,
        root: Hdc,
        images: &HashMap<String, DecodedImage>,
        scroll_y: i32,
        toolbar: i32,
        scale: f32,
        content: &Rect,
        dirty: &Rect,
    ) -> bool {
        let begin = matches!(
            item,
            DisplayItem::BeginOpacity { .. }
                | DisplayItem::BeginMask { .. }
                | DisplayItem::BeginTransform { .. }
        );
        let end = matches!(
            item,
            DisplayItem::EndOpacity { .. }
                | DisplayItem::EndMask { .. }
                | DisplayItem::EndTransform { .. }
        );
        if self.skipped > 0 {
            if begin {
                self.skipped += 1;
            } else if end {
                self.skipped -= 1;
            }
            return true;
        }
        let dc = self.dc(root);
        let bounds = match item {
            DisplayItem::BeginMask { bounds, .. } | DisplayItem::BeginOpacity { bounds, .. } => {
                Some(*bounds)
            }
            _ => None,
        };
        let bytes = bounds
            .and_then(|bounds| {
                super::opacity::intersection(
                    super::affine::device_bounds(
                        dc,
                        super::super::paint_primitives::screen_rect(
                            bounds, scroll_y, toolbar, scale,
                        ),
                    ),
                    *content,
                )
            })
            .and_then(|r| super::opacity::intersection(r, *dirty))
            .map_or(0, |r| r.width() as usize * r.height() as usize * 4);
        if begin && self.bytes.saturating_add(bytes) > 64 * 1024 * 1024 {
            self.skipped = 1;
            return true;
        }
        match item {
            DisplayItem::BeginTransform { matrix, .. } => {
                let matrix = super::affine::matrix(dc).then_local(super::affine::screen_matrix(
                    *matrix, scroll_y, toolbar, scale,
                ));
                let saved = SaveDC(dc);
                if saved == 0 || matrix.inverse().is_none() || !super::affine::set(dc, matrix) {
                    if saved != 0 {
                        RestoreDC(dc, saved);
                    }
                    self.skipped = 1;
                } else {
                    self.layers.push((Layer::Transform { dc, saved }, 0));
                }
            }
            DisplayItem::BeginOpacity { bounds, opacity } => {
                if *opacity <= 0.0 {
                    self.skipped = 1;
                } else {
                    match OpacityLayer::begin(
                        dc, *bounds, *opacity, scroll_y, toolbar, scale, content, dirty,
                    ) {
                        OpacityLayerStart::Hidden => self.skipped = 1,
                        OpacityLayerStart::Layer(layer) => {
                            self.layers.push((Layer::Opacity(layer), bytes));
                            self.bytes += bytes;
                        }
                    }
                }
            }
            DisplayItem::BeginMask { bounds, url } => {
                let layer = images.get(url).and_then(|mask| {
                    MaskLayer::begin(dc, *bounds, mask, scroll_y, toolbar, scale, content, dirty)
                });
                if let Some(layer) = layer {
                    self.layers.push((Layer::Mask(layer), bytes));
                    self.bytes += bytes;
                } else {
                    self.skipped = 1;
                }
            }
            DisplayItem::EndOpacity { .. }
            | DisplayItem::EndMask { .. }
            | DisplayItem::EndTransform { .. } => {
                if let Some((layer, bytes)) = self.layers.pop() {
                    self.bytes -= bytes;
                    match layer {
                        Layer::Opacity(v) => v.finish(),
                        Layer::Mask(v) => v.finish(),
                        Layer::Transform { dc, saved } => {
                            RestoreDC(dc, saved);
                        }
                    }
                }
            }
            _ => return false,
        }
        true
    }
}

impl Drop for PaintGroups {
    fn drop(&mut self) {
        // Balanced IPC ranges normally finish every layer. Keep GDI ownership
        // recoverable even if a traversal is interrupted during shutdown.
        while let Some((layer, _)) = self.layers.pop() {
            unsafe {
                match layer {
                    Layer::Opacity(v) => v.finish(),
                    Layer::Mask(v) => v.finish(),
                    Layer::Transform { dc, saved } => {
                        RestoreDC(dc, saved);
                    }
                }
            }
        }
    }
}
