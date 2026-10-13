//! Bounded paint snapshots from the private Canvas hook at rendering checkpoints.

use super::*;
use crate::limits::{MAX_CANVAS_PIXELS, MAX_PRESENTED_CANVASES};

pub(crate) struct CanvasPaintSnapshot {
    pub(crate) node: NodeId,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_size: (u32, u32),
    pub(crate) pixels: Option<Vec<u8>>,
    pub(crate) gpu_frame: Option<crate::gpu_frame::GpuFrame>,
}

fn unsigned(value: &JsValue) -> Option<u32> {
    let number = value.as_number()?;
    (number.is_finite() && number >= 0.0 && number <= f64::from(u32::MAX) && number.fract() == 0.0)
        .then_some(number as u32)
}

impl ScriptRuntime {
    pub(crate) fn set_gpu_canvas_enabled(&mut self, enabled: bool) {
        let mut host = self.host.borrow_mut();
        host.gpu_canvas_enabled = enabled;
        host.gpu_canvas_fallback = !enabled;
        if !enabled {
            host.timers.request_surface_repaint();
        }
    }
    pub(crate) fn take_canvas_presentation(&mut self) -> Result<Vec<CanvasPaintSnapshot>, String> {
        if !self.initialized {
            return Ok(Vec::new());
        }
        let Some(context) = self.context.as_deref_mut() else {
            return Ok(Vec::new());
        };
        if !self.host.borrow_mut().canvas_presentations.begin() {
            return Err("Canvas snapshot capture is already active".into());
        }
        let fallback = self.host.borrow().gpu_canvas_fallback;
        let response =
            context.call_global("__takeCanvasPresentation", &[JsValue::Boolean(fallback)]);
        // Always retire the transfer batch, including a hook exception or a
        // malformed reply. Unreferenced native pixels drop with this local batch.
        let mut native = self.host.borrow_mut().canvas_presentations.finish();
        let response = response.map_err(|error| format!("snapshot Canvas pixels: {error}"))?;
        let JsValue::Array(records) = response else {
            return Err("Canvas snapshot hook returned an invalid batch".into());
        };
        if records.len() > MAX_PRESENTED_CANVASES {
            return Err("Canvas snapshot batch exceeds the surface limit".into());
        }
        let mut snapshots = Vec::with_capacity(records.len());
        let mut total_bytes = 0_usize;
        for record in records {
            let JsValue::Array(fields) = record else {
                return Err("Canvas snapshot record is not an array".into());
            };
            let [handle, width, height, content_width, content_height, pixels]: [JsValue; 6] =
                fields
                    .try_into()
                    .map_err(|_| "Canvas snapshot record has the wrong shape")?;
            let (Some(handle), Some(width), Some(height)) =
                (unsigned(&handle), unsigned(&width), unsigned(&height))
            else {
                return Err("Canvas snapshot has invalid dimensions or node handle".into());
            };
            let content_size = unsigned(&content_width)
                .zip(unsigned(&content_height))
                .ok_or("Canvas snapshot has invalid content dimensions")?;
            let node = self
                .host
                .borrow()
                .node(handle)
                .filter(|node| node.tag_name() == Some("canvas"))
                .ok_or_else(|| "Canvas snapshot has no matching element".to_string())?;
            let mut gpu_frame = None;
            let pixels = if matches!(pixels, JsValue::Number(_)) {
                let token =
                    unsigned(&pixels).ok_or("Canvas snapshot has an invalid transfer token")?;
                gpu_frame = native.take_gpu(token, node.id(), width, height);
                if gpu_frame.is_some() {
                    JsValue::Null
                } else {
                    JsValue::Bytes(
                        native
                            .take(token, node.id(), width, height)
                            .ok_or("Canvas snapshot has no matching native transfer")?,
                    )
                }
            } else {
                pixels
            };
            let bytes = match pixels {
                JsValue::Null => None,
                JsValue::Bytes(value) => {
                    let count = (width as usize)
                        .checked_mul(height as usize)
                        .ok_or_else(|| "Canvas bitmap dimensions overflow".to_string())?;
                    if count == 0 || count > MAX_CANVAS_PIXELS || value.len() != count * 4 {
                        return Err("Canvas snapshot exceeds its pixel limit".into());
                    }
                    total_bytes = total_bytes
                        .checked_add(value.len())
                        .ok_or_else(|| "Canvas snapshot byte count overflow".to_string())?;
                    if total_bytes > crate::limits::MAX_PAGE_DECODED_IMAGE_BYTES {
                        return Err("Canvas snapshots exceed the page image budget".into());
                    }
                    Some(value)
                }
                _ => return Err("Canvas snapshot pixels have an invalid type".into()),
            };
            snapshots.push(CanvasPaintSnapshot {
                node: node.id(),
                width,
                height,
                content_size,
                pixels: bytes,
                gpu_frame,
            });
        }
        Ok(snapshots)
    }
}
