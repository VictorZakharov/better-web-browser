//! Whitelisted extension entry points from the existing ANGLE backend.
//! Signatures follow ANGLE's pinned GLES2/gl2ext.h and gl2ext_angle.h. No author
//! supplied name, native pointer or extension outside our implemented set is loaded.
use super::gl;
use super::texture_capabilities::TextureCapabilities;
use mozangle::egl::ffi as egl;
use std::ffi::{CStr, c_void};

pub(super) type DrawArrays = unsafe extern "system" fn(u32, i32, i32, i32);
pub(super) type DrawElements = unsafe extern "system" fn(u32, i32, u32, *const c_void, i32);
pub(super) type Divisor = unsafe extern "system" fn(u32, u32);
type RequestExtension = unsafe extern "system" fn(*const i8);
pub(super) type GenArrays = unsafe extern "system" fn(i32, *mut u32);
pub(super) type DeleteArrays = unsafe extern "system" fn(i32, *const u32);
pub(super) type BindArray = unsafe extern "system" fn(u32);
pub(super) type IsArray = unsafe extern "system" fn(u32) -> u8;
pub(super) type DrawBuffers = unsafe extern "system" fn(i32, *const u32);

pub(super) struct Extensions {
    pub textures: TextureCapabilities,
    pub arrays: Option<DrawArrays>,
    pub elements: Option<DrawElements>,
    pub divisor: Option<Divisor>,
    request: Option<RequestExtension>,
    pub available_instancing: bool,
    pub instancing: bool,
    pub gen_arrays: Option<GenArrays>,
    pub bind_array: Option<BindArray>,
    pub is_array: Option<IsArray>,
    pub available_vertex_arrays: bool,
    pub vertex_arrays: bool,
    pub available_uint_indices: bool,
    pub available_derivatives: bool,
    pub uint_indices: bool,
    pub derivatives: bool,
    pub available_frag_depth: bool,
    pub available_texture_lod: bool,
    pub frag_depth: bool,
    pub texture_lod: bool,
    pub available_draw_buffers: bool,
    pub draw_buffers: bool,
    pub draw_buffers_entry: Option<DrawBuffers>,
    pub max_draw_buffers: u32,
    pub max_color_attachments: u32,
}

macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        // SAFETY: a fixed ANGLE symbol with the exact pinned Khronos ABI. EGL
        // resolves only our linked backend; absence disables the whole extension.
        let pointer = unsafe { egl::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            None
        } else {
            Some(unsafe {
                std::mem::transmute::<egl::types::__eglMustCastToProperFunctionPointerType, $kind>(
                    pointer,
                )
            })
        }
    }};
}

impl Extensions {
    pub(super) fn new() -> Self {
        let enabled = extension_string(gl::EXTENSIONS);
        let request = if enabled.contains(&"GL_ANGLE_request_extension".into()) {
            entry!(c"glRequestExtensionANGLE", RequestExtension)
        } else {
            None
        };
        let requestable = if request.is_some() {
            extension_string(0x93a8)
        } else {
            Vec::new()
        };
        let advertised = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_ANGLE_instanced_arrays");
        let arrays = entry!(c"glDrawArraysInstancedANGLE", DrawArrays);
        let elements = entry!(c"glDrawElementsInstancedANGLE", DrawElements);
        let divisor = entry!(c"glVertexAttribDivisorANGLE", Divisor);
        let gen_arrays = entry!(c"glGenVertexArraysOES", GenArrays);
        let delete_arrays = entry!(c"glDeleteVertexArraysOES", DeleteArrays);
        let bind_array = entry!(c"glBindVertexArrayOES", BindArray);
        let is_array = entry!(c"glIsVertexArrayOES", IsArray);
        let vertex_arrays = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_OES_vertex_array_object");
        let available_uint_indices = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_OES_element_index_uint");
        let available_derivatives = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_OES_standard_derivatives");
        let available_frag_depth = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_EXT_frag_depth");
        let available_texture_lod = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_EXT_shader_texture_lod");
        let draw_buffers_entry = entry!(c"glDrawBuffersEXT", DrawBuffers);
        let advertised_draw_buffers = enabled
            .iter()
            .chain(&requestable)
            .any(|name| name == "GL_EXT_draw_buffers");
        let mut max_draw_buffers = 0;
        let mut max_color_attachments = 0;
        // Private draw-buffers admission supplies scalar capability queries;
        // public queries still require explicit author extension admission.
        unsafe {
            gl::GetIntegerv(0x8824, &mut max_draw_buffers);
            gl::GetIntegerv(0x8cdf, &mut max_color_attachments);
        }
        let available_draw_buffers = advertised_draw_buffers
            && draw_buffers_entry.is_some()
            && max_draw_buffers >= 4
            && max_color_attachments >= max_draw_buffers;
        Self {
            textures: TextureCapabilities::discover(
                enabled.iter().chain(&requestable).map(String::as_str),
            ),
            available_instancing: advertised
                && arrays.is_some()
                && elements.is_some()
                && divisor.is_some(),
            arrays,
            elements,
            divisor,
            request,
            instancing: false,
            available_vertex_arrays: vertex_arrays
                && gen_arrays.is_some()
                && delete_arrays.is_some()
                && bind_array.is_some()
                && is_array.is_some(),
            gen_arrays,
            bind_array,
            is_array,
            vertex_arrays: false,
            available_uint_indices,
            available_derivatives,
            uint_indices: false,
            derivatives: false,
            available_frag_depth,
            available_texture_lod,
            frag_depth: false,
            texture_lod: false,
            available_draw_buffers,
            draw_buffers: false,
            draw_buffers_entry,
            max_draw_buffers: max_draw_buffers.clamp(0, 16) as u32,
            max_color_attachments: max_color_attachments.clamp(0, 16) as u32,
        }
    }
    pub(super) fn enable_vertex_arrays(&mut self) -> bool {
        if !self.available_vertex_arrays {
            return false;
        }
        if !self.vertex_arrays {
            if let Some(request) = self.request {
                unsafe {
                    request(c"GL_OES_vertex_array_object".as_ptr());
                }
            }
            self.vertex_arrays = extension_string(gl::EXTENSIONS)
                .iter()
                .any(|name| name == "GL_OES_vertex_array_object");
        }
        self.vertex_arrays
    }
    pub(super) fn enable_instancing(&mut self) -> bool {
        if !self.available_instancing {
            return false;
        }
        if !self.instancing {
            if let Some(request) = self.request {
                unsafe {
                    request(c"GL_ANGLE_instanced_arrays".as_ptr());
                }
            }
            self.instancing = extension_string(gl::EXTENSIONS)
                .iter()
                .any(|name| name == "GL_ANGLE_instanced_arrays");
        }
        self.instancing
    }
    pub(super) fn enable_simple(&mut self, name: &CStr, available: bool) -> bool {
        if !available {
            return false;
        }
        // Some baseline extensions (notably ANGLE_depth_texture) are already
        // enabled and not requestable. Re-requesting them generates an error.
        if extension_string(gl::EXTENSIONS)
            .iter()
            .any(|value| value.as_bytes() == name.to_bytes())
        {
            return true;
        }
        if let Some(request) = self.request {
            unsafe {
                request(name.as_ptr());
            }
        }
        extension_string(gl::EXTENSIONS)
            .iter()
            .any(|value| value.as_bytes() == name.to_bytes())
    }
}

fn extension_string(pname: u32) -> Vec<String> {
    // Trusted driver-owned, NUL-terminated string, never an author pointer.
    let pointer = unsafe { gl::GetString(pname) };
    if pointer.is_null() {
        return Vec::new();
    }
    let text = unsafe { CStr::from_ptr(pointer.cast()) }.to_bytes();
    if text.len() > 32 * 1024 {
        return Vec::new();
    }
    std::str::from_utf8(text)
        .unwrap_or("")
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect()
}

pub(super) fn initialize_storage() -> std::result::Result<(), String> {
    // These private native format dependencies do not expose any author API.
    // Request fixed private format dependencies before surface allocation.
    // Native availability never grants the corresponding author extension.
    let enabled = extension_string(gl::EXTENSIONS);
    let request = entry!(c"glRequestExtensionANGLE", RequestExtension)
        .ok_or_else(|| "ANGLE extension request entry point unavailable".to_owned())?;
    let available = extension_string(0x93a8);
    for name in [
        c"GL_OES_rgb8_rgba8",
        c"GL_EXT_texture_storage",
        c"GL_OES_depth24",
        c"GL_EXT_draw_buffers",
    ] {
        if enabled
            .iter()
            .any(|value| value.as_bytes() == name.to_bytes())
        {
            continue;
        }
        if !available
            .iter()
            .any(|value| value.as_bytes() == name.to_bytes())
        {
            return Err(format!(
                "ANGLE native storage capability unavailable: {}",
                name.to_string_lossy()
            ));
        }
        unsafe {
            request(name.as_ptr());
        }
        if unsafe { gl::GetError() } != gl::NO_ERROR
            || !extension_string(gl::EXTENSIONS)
                .iter()
                .any(|value| value.as_bytes() == name.to_bytes())
        {
            return Err(format!(
                "ANGLE native storage capability request failed: {}",
                name.to_string_lossy()
            ));
        }
    }
    Ok(())
}
pub(super) unsafe fn delete_vertex_array(name: u32) {
    if let Some(delete) = entry!(c"glDeleteVertexArraysOES", DeleteArrays) {
        unsafe {
            delete(1, &name);
        }
    }
}

#[cfg(test)]
mod texture_tests {
    use super::super::texture_capabilities::TextureCapability;
    use super::*;

    #[test]
    fn pinned_warp_supports_the_texture_render_target_foundations() {
        super::super::session::run_native_test(|| {
            let _context = super::super::context::NativeContext::new().unwrap();
            let mut extensions = Extensions::new();
            let mut names = extension_string(gl::EXTENSIONS);
            names.extend(extension_string(0x93a8));
            for capability in TextureCapability::ALL {
                assert!(!extensions.textures.enabled(capability));
            }
            for capability in TextureCapability::ALL {
                assert!(
                    extensions.textures.available(capability),
                    "missing {capability:?}; native capabilities: {names:?}"
                );
                assert!(
                    extensions.enable_texture(capability),
                    "request failed {capability:?}"
                );
                assert!(extensions.textures.enabled(capability));
                assert_eq!(unsafe { gl::GetError() }, gl::NO_ERROR, "{capability:?}");
            }
        });
    }
}
