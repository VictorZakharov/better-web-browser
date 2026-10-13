//! One bounded, embedder-owned transfer from the GPU owner to a paint checkpoint.
//! Tokens carry no pixels into V8 and are valid only in the captured hook's batch.
use crate::engine::dom::NodeId;
use crate::limits::{MAX_CANVAS_PIXELS, MAX_PRESENTED_CANVASES};

const MAX_BYTES: usize = 64 * 1024 * 1024;

struct Record {
    token: u32,
    owner: NodeId,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    gpu: Option<crate::gpu_frame::GpuFrame>,
}

#[derive(Default)]
pub(in crate::engine::script) struct Transfers {
    active: bool,
    next: u32,
    bytes: usize,
    records: Vec<Record>,
}

pub(crate) struct Batch(Vec<Record>);

impl Transfers {
    pub(in crate::engine::script) fn begin(&mut self) -> bool {
        if self.active {
            return false;
        }
        self.active = true;
        true
    }

    pub(in crate::engine::script) fn available(&self) -> bool {
        self.active
            && self.records.len() < MAX_PRESENTED_CANVASES
            && self.bytes < MAX_BYTES
            && self.next < u32::MAX
    }

    pub(in crate::engine::script) fn record(
        &mut self,
        owner: NodeId,
        bitmap: (u32, u32, Vec<u8>),
    ) -> Option<(u32, u32, u32)> {
        let (width, height, pixels) = bitmap;
        let count = (width as usize).checked_mul(height as usize)?;
        let bytes = self.bytes.checked_add(pixels.capacity())?;
        if !self.available()
            || count == 0
            || count > MAX_CANVAS_PIXELS
            || pixels.len() != count * 4
            || bytes > MAX_BYTES
        {
            return None;
        }
        self.next += 1;
        self.bytes = bytes;
        self.records.push(Record {
            token: self.next,
            owner,
            width,
            height,
            pixels,
            gpu: None,
        });
        Some((width, height, self.next))
    }

    pub(in crate::engine::script) fn record_gpu(
        &mut self,
        owner: NodeId,
        gpu: crate::gpu_frame::GpuFrame,
    ) -> Option<(u32, u32, u32)> {
        let bytes = (gpu.width as usize)
            .checked_mul(gpu.height as usize)?
            .checked_mul(4)?
            .checked_add(self.bytes)?;
        if !self.available() || !gpu.validate() || bytes > MAX_BYTES {
            return None;
        }
        self.next += 1;
        self.bytes = bytes;
        self.records.push(Record {
            token: self.next,
            owner,
            width: gpu.width,
            height: gpu.height,
            pixels: Vec::new(),
            gpu: Some(gpu),
        });
        Some((gpu.width, gpu.height, self.next))
    }

    pub(in crate::engine::script) fn finish(&mut self) -> Batch {
        self.active = false;
        self.bytes = 0;
        Batch(std::mem::take(&mut self.records))
    }
}

impl Batch {
    pub(crate) fn take_gpu(
        &mut self,
        token: u32,
        owner: NodeId,
        width: u32,
        height: u32,
    ) -> Option<crate::gpu_frame::GpuFrame> {
        let index = self.0.iter().position(|record| {
            record.token == token
                && record.owner == owner
                && record.width == width
                && record.height == height
                && record.gpu.is_some()
        })?;
        self.0.swap_remove(index).gpu
    }
    pub(crate) fn take(
        &mut self,
        token: u32,
        owner: NodeId,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let index = self.0.iter().position(|record| {
            record.token == token
                && record.owner == owner
                && record.width == width
                && record.height == height
                && record.gpu.is_none()
        })?;
        Some(self.0.swap_remove(index).pixels)
    }
}

#[cfg(test)]
mod tests;
