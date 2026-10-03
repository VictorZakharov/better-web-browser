//! Author filtering admission can be narrower than the native provider.
//! Incomplete WebGL textures sample opaque black, rather than generating a
//! draw error or acquiring a provider's broader filtering capabilities.
use super::texture_capabilities::TextureCapability;
use super::{Kind, Result, WebGl, gl};

pub(super) struct SamplingGuard {
    bindings: Vec<(u32, u32, u32)>,
    active: u32,
    touched: bool,
}

impl WebGl {
    pub(super) fn sampling_guard(&self) -> Result<SamplingGuard> {
        let mut guard = SamplingGuard {
            bindings: Vec::new(),
            active: gl::TEXTURE0 + self.texture_unit as u32,
            touched: false,
        };
        let linear_half = self
            .extensions
            .textures
            .enabled(TextureCapability::HalfFloatLinear);
        if linear_half {
            return Ok(guard);
        }
        for (unit, bindings) in self.textures.iter().enumerate() {
            for (slot, &id) in bindings.iter().enumerate().filter(|(_, id)| **id != 0) {
                let object = self.objects.get(id, Kind::Texture)?;
                if !object.texture_images.iter().any(|((_, level), (_, kind))| {
                    *level == 0 && *kind == super::texture_formats::HALF_FLOAT
                }) {
                    continue;
                }
                let target = if slot == 0 {
                    gl::TEXTURE_2D
                } else {
                    gl::TEXTURE_CUBE_MAP
                };
                let mut min = 0;
                let mut mag = 0;
                guard.touched = true;
                // SAFETY: fixed targets and bounded registered texture units.
                unsafe {
                    gl::ActiveTexture(gl::TEXTURE0 + unit as u32);
                    gl::GetTexParameteriv(target, gl::TEXTURE_MIN_FILTER, &mut min);
                    gl::GetTexParameteriv(target, gl::TEXTURE_MAG_FILTER, &mut mag);
                }
                if mag != gl::NEAREST as i32
                    || ![gl::NEAREST as i32, gl::NEAREST_MIPMAP_NEAREST as i32].contains(&min)
                {
                    guard
                        .bindings
                        .push((gl::TEXTURE0 + unit as u32, target, object.native));
                    // Texture zero has no images. ANGLE supplies WebGL's robust
                    // incomplete-texture black value; no private pixel allocation.
                    unsafe {
                        gl::BindTexture(target, 0);
                    }
                }
            }
        }
        if guard.touched {
            unsafe {
                gl::ActiveTexture(guard.active);
            }
        }
        Ok(guard)
    }
}

impl Drop for SamplingGuard {
    fn drop(&mut self) {
        if !self.touched {
            return;
        }
        // Restore even when the draw or a later validation fails. The guard is
        // owner-thread-local and cannot outlive its synchronous native draw.
        unsafe {
            for &(unit, target, native) in &self.bindings {
                gl::ActiveTexture(unit);
                gl::BindTexture(target, native);
            }
            gl::ActiveTexture(self.active);
        }
    }
}
