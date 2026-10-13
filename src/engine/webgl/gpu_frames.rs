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
mod copy;
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
}
pub(super) struct Pool {
    slots: Vec<Slot>,
    width: u32,
    height: u32,
    adapter_low: u32,
    adapter_high: i32,
    latest: Option<usize>,
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
            });
        }
        Ok(Self {
            slots,
            width,
            height,
            adapter_low: desc.AdapterLuid.LowPart,
            adapter_high: desc.AdapterLuid.HighPart,
            latest: None,
            _charge: charge,
            copy,
        })
    }
}

#[path = "gpu_frames/fallback.rs"]
mod fallback;

impl WebGl {
    pub(super) fn gpu_frame(&mut self) -> Result<Option<GpuFrame>, String> {
        let extent = (self.surface.width, self.surface.height);
        if self.gpu_unavailable_extent == Some(extent) {
            return Ok(None);
        }
        let result = self.export_gpu_frame();
        if result.is_err() {
            self.gpu_unavailable_extent = Some(extent);
        }
        result
    }

    fn export_gpu_frame(&mut self) -> Result<Option<GpuFrame>, String> {
        if self.options.api != super::ApiVersion::Two || self.objects.poisoned {
            return Ok(None);
        }
        self.native.make_current()?;
        if self.gpu_frames.as_ref().is_some_and(|pool| {
            pool.width != self.surface.width || pool.height != self.surface.height
        }) {
            self.gpu_frames = None;
            self.native.make_current()?;
        }
        if self.gpu_frames.is_none() {
            self.gpu_frames = Some(Pool::new(self)?);
        }
        let pool = self.gpu_frames.as_mut().unwrap();
        if let Some(source) = self.surface.presentation_resolve_source() {
            unsafe {
                gl::Flush();
            }
            let resolve = pool.copy.resolve(
                source,
                self.surface.presentation_texture(),
                pool.width,
                pool.height,
            );
            let restored = self.native.make_current();
            resolve?;
            restored?;
            self.surface.mark_presentation_resolved();
        }
        for index in 0..pool.slots.len() {
            // Keep the latest completed frame intact for a later bitmap fallback.
            if pool.latest == Some(index) {
                continue;
            }
            let slot = &mut pool.slots[index];
            let next_key = slot.key.checked_add(1).ok_or("GPU frame key exhausted")?;
            let acquired = try_acquire(&slot.surface.mutex, 0)?
                || (slot.key != 0 && try_acquire(&slot.surface.mutex, slot.key)?);
            if !acquired {
                continue;
            }
            unsafe {
                gl::Flush();
            }
            let copy = pool.copy.copy(
                &slot.surface,
                self.surface.presentation_texture(),
                pool.width,
                pool.height,
                self.options.alpha,
                self.options.premultiplied_alpha,
            );
            let restored = self.native.make_current();
            if copy.is_err() || restored.is_err() {
                unsafe { slot.surface.mutex.ReleaseSync(0) }.map_err(|error| error.to_string())?;
                copy?;
                restored?;
            }
            unsafe { slot.surface.mutex.ReleaseSync(next_key) }
                .map_err(|error| error.to_string())?;
            slot.key = next_key;
            pool.latest = Some(index);
            return Ok(Some(GpuFrame {
                surface: slot.id,
                handle: slot.surface.handle().0 as usize as u64,
                key: slot.key,
                width: pool.width,
                height: pool.height,
                adapter_low: pool.adapter_low,
                adapter_high: pool.adapter_high,
            }));
        }
        Ok(None)
    }
}
