//! Fixed GLES3 entry points from the linked ANGLE provider.
//! ABIs follow the pinned Khronos GLES3/gl3.h; no author symbol is resolved.
use mozangle::egl::ffi as egl;
use std::ffi::c_void;

pub(super) type CopyBuffer = unsafe extern "system" fn(u32, u32, isize, isize, isize);
pub(super) type MapBuffer = unsafe extern "system" fn(u32, isize, isize, u32) -> *mut c_void;
pub(super) type UnmapBuffer = unsafe extern "system" fn(u32) -> u8;
pub(super) type UnsignedUniform = unsafe extern "system" fn(i32, i32, *const u32);
pub(super) type MatrixUniform = unsafe extern "system" fn(i32, i32, u8, *const f32);
pub(super) type GetUnsignedUniform = unsafe extern "system" fn(u32, i32, *mut u32);

pub(super) struct CoreEntries {
    pub copy_buffer: CopyBuffer,
    pub map_buffer: MapBuffer,
    pub unmap_buffer: UnmapBuffer,
    pub unsigned_uniform: [UnsignedUniform; 4],
    pub matrix_uniform: [MatrixUniform; 6],
    pub get_unsigned_uniform: GetUnsignedUniform,
}

macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        // SAFETY: EGL resolves a compile-time symbol in our linked ANGLE library;
        // the exact Khronos ABI is fixed here, never derived from author input.
        let pointer = unsafe { egl::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            return Err(format!(
                "Required GLES3 entry point unavailable: {}",
                $name.to_string_lossy()
            ));
        }
        unsafe {
            std::mem::transmute::<egl::types::__eglMustCastToProperFunctionPointerType, $kind>(
                pointer,
            )
        }
    }};
}

impl CoreEntries {
    pub(super) fn load() -> Result<Self, String> {
        Ok(Self {
            copy_buffer: entry!(c"glCopyBufferSubData", CopyBuffer),
            map_buffer: entry!(c"glMapBufferRange", MapBuffer),
            unmap_buffer: entry!(c"glUnmapBuffer", UnmapBuffer),
            unsigned_uniform: [
                entry!(c"glUniform1uiv", UnsignedUniform),
                entry!(c"glUniform2uiv", UnsignedUniform),
                entry!(c"glUniform3uiv", UnsignedUniform),
                entry!(c"glUniform4uiv", UnsignedUniform),
            ],
            matrix_uniform: [
                entry!(c"glUniformMatrix2x3fv", MatrixUniform),
                entry!(c"glUniformMatrix2x4fv", MatrixUniform),
                entry!(c"glUniformMatrix3x2fv", MatrixUniform),
                entry!(c"glUniformMatrix3x4fv", MatrixUniform),
                entry!(c"glUniformMatrix4x2fv", MatrixUniform),
                entry!(c"glUniformMatrix4x3fv", MatrixUniform),
            ],
            get_unsigned_uniform: entry!(c"glGetUniformuiv", GetUnsignedUniform),
        })
    }
}
