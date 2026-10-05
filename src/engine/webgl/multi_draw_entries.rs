//! Closed ANGLE_multi_draw entry points, matching the pinned gl2ext_angle.h ABI.
use mozangle::egl::ffi as egl;
use std::ffi::c_void;

type Arrays = unsafe extern "system" fn(u32, *const i32, *const i32, i32);
type ArraysInstanced = unsafe extern "system" fn(u32, *const i32, *const i32, *const i32, i32);
type Elements = unsafe extern "system" fn(u32, *const i32, u32, *const *const c_void, i32);
type ElementsInstanced =
    unsafe extern "system" fn(u32, *const i32, u32, *const *const c_void, *const i32, i32);

#[derive(Clone, Copy)]
pub(super) struct Entries {
    pub arrays: Arrays,
    pub arrays_instanced: ArraysInstanced,
    pub elements: Elements,
    pub elements_instanced: ElementsInstanced,
}

macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        // SAFETY: only fixed symbols from our linked ANGLE provider, with the
        // exact native ABI. Missing any entry disables the entire extension.
        let pointer = unsafe { egl::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            return None;
        }
        unsafe {
            std::mem::transmute::<egl::types::__eglMustCastToProperFunctionPointerType, $kind>(
                pointer,
            )
        }
    }};
}

impl Entries {
    pub fn load() -> Option<Self> {
        Some(Self {
            arrays: entry!(c"glMultiDrawArraysANGLE", Arrays),
            arrays_instanced: entry!(c"glMultiDrawArraysInstancedANGLE", ArraysInstanced),
            elements: entry!(c"glMultiDrawElementsANGLE", Elements),
            elements_instanced: entry!(c"glMultiDrawElementsInstancedANGLE", ElementsInstanced),
        })
    }
}
