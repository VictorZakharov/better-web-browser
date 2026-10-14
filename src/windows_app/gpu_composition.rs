//! GPU page composition owned by one native window; browser chrome remains native UI.
mod backgrounds;
mod borders;
mod coverage;
mod groups;
mod images;
mod integration;
mod scheduling;
pub(super) const RETRY_TIMER: usize = 27;
mod leaves;
mod painting;
mod presentation;
mod primitives;
mod shapes;

use super::*;
use better_web_browser::gpu_composition::{Device, Fence, Frames, Target};
use better_web_browser::renderer_protocol::{DocumentId, PresentedGpuCanvas};
use windows::{
    Win32::Graphics::{Direct2D::Common::*, Direct2D::*},
    core::Interface,
};

pub(super) struct Compositor {
    device: Device,
    frames: Frames,
    target: Target,
    images: images::Bitmaps,
    coverage: coverage::Coverage,
    presentation: presentation::Presentation,
    document: DocumentId,
    active: bool,
    inflight: std::collections::VecDeque<Fence>,
}

impl Compositor {
    fn new(
        window: Hwnd,
        document: DocumentId,
        frame: &PresentedGpuCanvas,
        width: u32,
        height: u32,
        top: i32,
        budget: better_web_browser::renderer_budget::RendererBudget,
    ) -> Result<Self, String> {
        let device = Device::hardware(Some((frame.frame.adapter_low, frame.frame.adapter_high)))?;
        let target = Target::new(&device, width, height)?;
        let presentation = presentation::Presentation::new(&device, window, width, height, top)?;
        Ok(Self {
            device,
            frames: Frames::default(),
            target,
            images: images::Bitmaps::new(budget),
            coverage: coverage::Coverage::default(),
            presentation,
            document,
            active: false,
            inflight: std::collections::VecDeque::new(),
        })
    }

    fn resize(&mut self, width: u32, height: u32, top: i32) -> Result<(), String> {
        if (self.target.width, self.target.height) != (width, height) {
            unsafe {
                self.device.d2d.SetTarget(None);
            }
            self.target = Target::new(&self.device, width, height)?;
            self.presentation.resize(width, height)?;
        }
        self.presentation.position(top)
    }

    pub(super) fn is_active(&self, document: Option<DocumentId>) -> bool {
        self.active && self.belongs_to(document)
    }
    pub(super) fn belongs_to(&self, document: Option<DocumentId>) -> bool {
        document == Some(self.document)
    }
    fn hide(&mut self) -> Result<(), String> {
        self.presentation.hide()?;
        self.active = false;
        Ok(())
    }
    pub(super) fn capture_bgra(&self) -> Result<(u32, u32, Vec<u8>), String> {
        Ok((
            self.target.width,
            self.target.height,
            self.target.read_bgra(&self.device)?,
        ))
    }

    fn queue_available(&mut self, width: u32, height: u32) -> Result<bool, String> {
        while let Some(fence) = self.inflight.front() {
            if !fence.complete(&self.device)? {
                break;
            }
            self.inflight.pop_front();
        }
        // ResizeBuffers must not synchronize the HWND thread to old submissions.
        Ok(
            if (self.target.width, self.target.height) != (width, height) {
                self.inflight.is_empty()
            } else {
                self.inflight.len() < 2
            },
        )
    }
}
