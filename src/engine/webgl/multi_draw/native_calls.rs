//! Preserve ordinary gl_DrawID=0 with the pinned ANGLE provider.
//! Its multi-draw implementation leaves the private uniform at the last ID.
//! A one-entry native batch resets it without touching shader text or uniforms.
use crate::engine::webgl::{WebGl, gl};
use std::ffi::c_void;

impl WebGl {
    pub(in crate::engine::webgl) fn native_draw_arrays(
        &self,
        mode: u32,
        first: i32,
        count: i32,
        instances: Option<i32>,
    ) {
        // SAFETY: caller has completed ordinary WebGL validation. Stack-owned
        // scalar lists live across these synchronous exact-ABI native calls.
        unsafe {
            if let Some(entries) = self
                .extensions
                .multi_draw_entries
                .filter(|_| self.extensions.multi_draw)
            {
                if let Some(instances) = instances {
                    (entries.arrays_instanced)(mode, &first, &count, &instances, 1);
                } else {
                    (entries.arrays)(mode, &first, &count, 1);
                }
            } else if let Some(instances) = instances {
                (self.extensions.arrays.expect("admitted instancing"))(
                    mode, first, count, instances,
                );
            } else {
                gl::DrawArrays(mode, first, count);
            }
        }
    }

    pub(in crate::engine::webgl) fn native_draw_elements(
        &self,
        mode: u32,
        count: i32,
        kind: u32,
        offset: usize,
        instances: Option<i32>,
    ) {
        let offset = offset as *const c_void;
        // SAFETY: caller has checked the bound element-buffer range, alignment
        // and vertex/instance buffers. Offset is not an author native address.
        unsafe {
            if let Some(entries) = self
                .extensions
                .multi_draw_entries
                .filter(|_| self.extensions.multi_draw)
            {
                if let Some(instances) = instances {
                    (entries.elements_instanced)(mode, &count, kind, &offset, &instances, 1);
                } else {
                    (entries.elements)(mode, &count, kind, &offset, 1);
                }
            } else if let Some(instances) = instances {
                (self.extensions.elements.expect("admitted instancing"))(
                    mode, count, kind, offset, instances,
                );
            } else {
                gl::DrawElements(mode, count, kind, offset);
            }
        }
    }
}
