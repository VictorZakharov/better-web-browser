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
pub(super) type IntegerPointer = unsafe extern "system" fn(u32, i32, u32, i32, *const c_void);
pub(super) type IntegerAttribute = unsafe extern "system" fn(u32, *const i32);
pub(super) type UnsignedAttribute = unsafe extern "system" fn(u32, *const u32);
pub(super) type GetIntegerAttribute = unsafe extern "system" fn(u32, u32, *mut i32);
pub(super) type GetUnsignedAttribute = unsafe extern "system" fn(u32, u32, *mut u32);
pub(super) type GetInteger64 = unsafe extern "system" fn(u32, *mut i64);
pub(super) type TextureStorage2D = unsafe extern "system" fn(u32, i32, u32, i32, i32);
pub(super) type TextureStorage3D = unsafe extern "system" fn(u32, i32, u32, i32, i32, i32);
pub(super) type TextureImage3D =
    unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
pub(super) type TextureSubImage3D =
    unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, i32, i32, u32, u32, *const c_void);

pub(super) struct CoreEntries {
    pub texture_storage_3d: TextureStorage3D,
    pub texture_image_3d: TextureImage3D,
    pub texture_sub_image_3d: TextureSubImage3D,
    pub texture_storage_2d: TextureStorage2D,
    pub get_integer64: GetInteger64,
    pub max_element_index: u32,
    pub integer_pointer: IntegerPointer,
    pub integer_attribute: IntegerAttribute,
    pub unsigned_attribute: UnsignedAttribute,
    pub get_integer_attribute: GetIntegerAttribute,
    pub get_unsigned_attribute: GetUnsignedAttribute,
    pub gen_arrays: super::extensions::GenArrays,
    pub bind_array: super::extensions::BindArray,
    pub is_array: super::extensions::IsArray,
    pub arrays: super::extensions::DrawArrays,
    pub elements: super::extensions::DrawElements,
    pub divisor: super::extensions::Divisor,
    pub draw_buffers: super::extensions::DrawBuffers,
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
        // Teardown resolves this same fixed symbol; fail creation if it is absent.
        let _: super::extensions::DeleteArrays =
            entry!(c"glDeleteVertexArrays", super::extensions::DeleteArrays);
        let get_integer64 = entry!(c"glGetInteger64v", GetInteger64);
        let mut max_element_index = 0;
        // SAFETY: exact scalar GLint64 output for the fixed GLES3 capability.
        unsafe {
            get_integer64(0x8d6b, &mut max_element_index);
        }
        if unsafe { super::gl::GetError() } != super::gl::NO_ERROR
            || !(0x00ff_ffff..=u32::MAX as i64).contains(&max_element_index)
        {
            return Err("ANGLE MAX_ELEMENT_INDEX does not meet the admitted WebGL2 minimum".into());
        }
        Ok(Self {
            texture_storage_3d: entry!(c"glTexStorage3D", TextureStorage3D),
            texture_image_3d: entry!(c"glTexImage3D", TextureImage3D),
            texture_sub_image_3d: entry!(c"glTexSubImage3D", TextureSubImage3D),
            texture_storage_2d: entry!(c"glTexStorage2D", TextureStorage2D),
            get_integer64,
            max_element_index: max_element_index as u32,
            integer_pointer: entry!(c"glVertexAttribIPointer", IntegerPointer),
            integer_attribute: entry!(c"glVertexAttribI4iv", IntegerAttribute),
            unsigned_attribute: entry!(c"glVertexAttribI4uiv", UnsignedAttribute),
            get_integer_attribute: entry!(c"glGetVertexAttribIiv", GetIntegerAttribute),
            get_unsigned_attribute: entry!(c"glGetVertexAttribIuiv", GetUnsignedAttribute),
            gen_arrays: entry!(c"glGenVertexArrays", super::extensions::GenArrays),
            bind_array: entry!(c"glBindVertexArray", super::extensions::BindArray),
            is_array: entry!(c"glIsVertexArray", super::extensions::IsArray),
            arrays: entry!(c"glDrawArraysInstanced", super::extensions::DrawArrays),
            elements: entry!(c"glDrawElementsInstanced", super::extensions::DrawElements),
            divisor: entry!(c"glVertexAttribDivisor", super::extensions::Divisor),
            draw_buffers: entry!(c"glDrawBuffers", super::extensions::DrawBuffers),
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
