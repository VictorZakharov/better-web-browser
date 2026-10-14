//! GPU-only copies into bounded, immutable-until-handoff presentation slots.
use super::{WebGl, context::SharedSurface, gl};
use crate::gpu_frame::{GpuFrame, try_acquire};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use windows::{Win32::Graphics::Dxgi::IDXGIDevice, core::Interface};

static NEXT_SURFACE: AtomicU64 = AtomicU64::new(1);
static RESIDENT_BYTES: AtomicUsize = AtomicUsize::new(0);
const MAX_BYTES: usize = 64 * 1024 * 1024;
const SLOTS: usize = 3;
#[cfg(test)]
mod command_samples;
mod completion;
mod copy;
mod publication;
pub(crate) use publication::Export;
#[cfg(test)]
mod async_tests;
#[cfg(test)]
mod tests;

struct Charge(usize);
impl Charge {
    fn reserve(bytes: usize) -> Option<Self> {
        RESIDENT_BYTES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= MAX_BYTES)
            })
            .ok()
            .map(|_| Self(bytes))
    }
}
impl Drop for Charge {
    fn drop(&mut self) {
        RESIDENT_BYTES.fetch_sub(self.0, Ordering::Relaxed);
    }
}
struct Slot {
    surface: SharedSurface,
    id: u64,
    key: u64,
    completion: Option<completion::Completion>,
    sequence: u64,
}
pub(super) struct Pool {
    slots: Vec<Slot>,
    width: u32,
    height: u32,
    adapter_low: u32,
    adapter_high: i32,
    latest: Option<usize>,
    sequence: u64,
    _charge: Charge,
    copy: copy::Renderer,
}

impl Pool {
    fn new(context: &WebGl) -> Result<Self, String> {
        let width = context.surface.width;
        let height = context.surface.height;
        let bytes = width as usize * height as usize * 4 * SLOTS;
        let charge = Charge::reserve(bytes).ok_or("GPU frame slots exceed the renderer budget")?;
        let device = context.native.d3d_device()?;
        let dxgi: IDXGIDevice = device.cast().map_err(|error| error.to_string())?;
        let desc = unsafe { dxgi.GetAdapter().and_then(|adapter| adapter.GetDesc()) }
            .map_err(|error| error.to_string())?;
        let copy = copy::Renderer::new(&context.native)?;
        let mut slots = Vec::with_capacity(SLOTS);
        for _ in 0..SLOTS {
            let id = NEXT_SURFACE
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| "GPU surface identity exhausted")?;
            slots.push(Slot {
                surface: context.native.shared_surface(width, height)?,
                id,
                key: 0,
                completion: None,
                sequence: 0,
            });
        }
        Ok(Self {
            slots,
            width,
            height,
            adapter_low: desc.AdapterLuid.LowPart,
            adapter_high: desc.AdapterLuid.HighPart,
            latest: None,
            sequence: 0,
            _charge: charge,
            copy,
        })
    }
}

#[path = "gpu_frames/fallback.rs"]
mod fallback;
