//! Renderer-owned GLES2 contexts with a WebGL compatibility/zero-initialization boundary.
//! ANGLE owns GLSL parsing, translation and driver calls. Browser handles never expose pointers.
use mozangle::gles::ffi as gl;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

mod api_version;
#[cfg(test)]
mod api_version_tests;
mod buffers;
mod commands;
mod construction;
mod context;
mod copy_texture;
mod copy_texture_conversion;
mod core_attachment_queries;
#[cfg(test)]
mod core_attachment_tests;
mod core_attachments;
#[cfg(test)]
mod core_buffer_tests;
mod core_buffers;
#[cfg(test)]
mod core_draw_tests;
mod core_draws;
mod core_entries;
mod core_extensions;
#[cfg(test)]
mod core_framebuffer_tests;
mod core_framebuffers;
#[cfg(test)]
mod core_parameter_tests;
mod core_parameters;
mod core_renderbuffers;
#[cfg(test)]
mod core_texture_format_tests;
mod core_texture_formats;
#[cfg(test)]
mod core_texture_mip_tests;
mod core_texture_mips;
#[cfg(test)]
mod core_texture_tests;
mod core_textures;
#[cfg(test)]
mod core_uniform_tests;
mod core_uniforms;
mod depth_textures;
mod draw_buffers;
mod extension_commands;
mod extensions;
mod float_values;
mod framebuffer_attachments;
mod framebuffer_completeness;
mod framebuffer_guard;
#[cfg(test)]
mod framebuffer_guard_tests;
mod framebuffer_queries;
mod framebuffers;
mod index_ranges;
mod instancing;
mod multisample;
#[cfg(test)]
mod multisample_tests;
mod object_queries;
mod objects;
mod parameters;
mod pixel_buffer_guard;
#[cfg(test)]
mod pixel_buffer_tests;
mod pixel_readback;
mod pixel_transport;
mod presentation;
mod queries;
mod resize;
#[cfg(test)]
mod sampler_tests;
mod samplers;
mod session;
mod shader_commands;
mod shader_queries;
mod shader_validation;
mod stencil_masks;
mod surface;
#[cfg(test)]
mod tests;
mod texture_capabilities;
mod texture_color_space;
mod texture_formats;
mod texture_sampling;
mod texture_targets;
mod textures;
#[cfg(test)]
mod typed_framebuffer_tests;
mod typed_framebuffers;
mod uniform_queries;
#[cfg(test)]
mod uniform_validation_tests;
mod uniforms;
#[cfg(test)]
mod validation_tests;
mod vertex_arrays;
mod vertex_attribute_queries;
#[cfg(test)]
mod vertex_attribute_tests;
mod vertex_attributes;
mod volume_initialization;
#[cfg(test)]
mod volume_mip_tests;
#[cfg(test)]
mod volume_texture_tests;
mod volume_textures;
pub(crate) use pixel_transport::PixelReply;
pub(crate) use session::Contexts;

use api_version::ApiVersion;
use context::NativeContext;
use objects::{Kind, Objects};
use surface::Surface;

pub(crate) const MAX_CONTEXTS: usize = 8;
const MAX_OBJECTS: usize = 1024;
const MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PROCESS_RESOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_NATIVE_CONTEXTS: usize = 16;
const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_SHADER_BYTES: usize = 32 * 1024;
const MAX_DRAW_VERTICES: u32 = 1_000_000;
type Result<T> = std::result::Result<T, u32>;

/// The enclosing realm owns this registry and drops it on navigation/worker shutdown.
#[derive(Default)]
struct BackendContexts {
    contexts: HashMap<u32, WebGl>,
    next: u32,
}

impl BackendContexts {
    pub(crate) fn create(&mut self, width: u32, height: u32, options: &str) -> Option<u32> {
        if self.contexts.len() >= MAX_NATIVE_CONTEXTS {
            return None;
        }
        let options: Options = serde_json::from_str(options).ok()?;
        let surface_bytes = (width as usize).checked_mul(height as usize)?.checked_mul(
            if options.depth || options.stencil {
                8
            } else {
                4
            },
        )?;
        let existing: usize = self
            .contexts
            .values()
            .map(|context| context.resource_bytes)
            .sum();
        if existing.checked_add(surface_bytes)? > MAX_PROCESS_RESOURCE_BYTES {
            return None;
        }
        let next = self.next.checked_add(1)?;
        let context = WebGl::new(width, height, options).ok()?;
        self.next = next;
        self.contexts.insert(self.next, context);
        Some(self.next)
    }
    pub(crate) fn remove(&mut self, id: u32) {
        self.contexts.remove(&id);
    }
    pub(crate) fn execute(&mut self, id: u32, command: &str, bytes: Option<&[u8]>) -> Value {
        let other_bytes: usize = self
            .contexts
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, context)| context.resource_bytes)
            .sum();
        let Some(context) = self.contexts.get_mut(&id) else {
            return json!({"lost":true});
        };
        context.resource_limit =
            MAX_RESOURCE_BYTES.min(MAX_PROCESS_RESOURCE_BYTES.saturating_sub(other_bytes));
        if command.len() > MAX_SHADER_BYTES + 4096
            || bytes.is_some_and(|b| b.len() > MAX_UPLOAD_BYTES)
        {
            context.error(gl::OUT_OF_MEMORY);
            return Value::Null;
        }
        let command = match serde_json::from_str::<Command>(command) {
            Ok(command) => command,
            Err(_) => {
                context.error(gl::INVALID_VALUE);
                return Value::Null;
            }
        };
        if context.native.make_current().is_err() {
            return json!({"lost":true});
        }
        match context.dispatch(&command, bytes) {
            Ok(value) => value,
            Err(error) => {
                context.error(error);
                Value::Null
            }
        }
    }
    pub(crate) fn snapshot(&mut self, id: u32) -> Option<(u32, u32, Vec<u8>)> {
        let context = self.contexts.get_mut(&id)?;
        context.native.make_current().ok()?;
        let mut pixels = context.surface.snapshot().ok()?;
        if !context.options.alpha {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        Some((context.surface.width, context.surface.height, pixels))
    }
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Options {
    api: ApiVersion,
    alpha: bool,
    depth: bool,
    stencil: bool,
    preserve: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            api: ApiVersion::One,
            alpha: true,
            depth: true,
            stencil: false,
            preserve: false,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    op: String,
    #[serde(default)]
    i: Vec<i64>,
    #[serde(default, deserialize_with = "float_values::deserialize")]
    f: Vec<f64>,
    #[serde(default)]
    text: String,
}
impl Command {
    fn u(&self, index: usize) -> Result<u32> {
        self.i
            .get(index)
            .copied()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(gl::INVALID_VALUE)
    }
    fn n(&self, index: usize) -> Result<i32> {
        self.i
            .get(index)
            .copied()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(gl::INVALID_VALUE)
    }
    fn float(&self, index: usize) -> Result<f32> {
        self.f
            .get(index)
            .copied()
            .map(|n| n as f32)
            .ok_or(gl::INVALID_VALUE)
    }
}

struct WebGl {
    // Surface/resources must be deleted with their owning context current, before EGL teardown.
    native: NativeContext,
    core: Option<core_entries::CoreEntries>,
    core_buffer_bindings: HashMap<u32, u32>,
    surface: Surface,
    objects: Objects,
    errors: VecDeque<u32>,
    options: Options,
    stencil_masks: stencil_masks::StencilMasks,
    resource_bytes: usize,
    resource_limit: usize,
    array_buffer: u32,
    element_buffer: u32,
    program: u32,
    attributes: Vec<buffers::Attribute>,
    attribute_values: Vec<vertex_attributes::ValueKind>,
    extensions: extensions::Extensions,
    vertex_arrays: vertex_arrays::VertexArrays,
    framebuffer: u32,
    read_framebuffer: u32,
    default_read_buffer: u32,
    renderbuffer: u32,
    texture_unit: usize,
    textures: Vec<[u32; 4]>,
    samplers: Vec<u32>,
    default_draw_buffer: u32,
}
impl WebGl {
    fn error(&mut self, error: u32) {
        if error != gl::NO_ERROR && !self.errors.contains(&error) && self.errors.len() < 8 {
            self.errors.push_back(error);
        }
    }
    fn charge(&mut self, previous: usize, next: usize) -> Result<()> {
        // Lifetime high-water accounting: deleted resources may remain referenced by unbound
        // framebuffer/program/vertex state in GLES. Never reclaim their charge prematurely.
        // Buffer storage also has a CPU mirror for indexed-draw validation.
        let growth = next
            .saturating_sub(previous)
            .checked_mul(2)
            .ok_or(gl::OUT_OF_MEMORY)?;
        self.resource_bytes = self
            .resource_bytes
            .checked_add(growth)
            .filter(|n| *n <= self.resource_limit)
            .ok_or(gl::OUT_OF_MEMORY)?;
        Ok(())
    }
    fn driver_result(&mut self) -> Result<()> {
        // Drain into WebGL's per-context sticky error set, rather than losing an earlier error.
        let mut first = None;
        for _ in 0..8 {
            let error = unsafe { gl::GetError() };
            if error == gl::NO_ERROR {
                break;
            }
            first.get_or_insert(error);
            self.error(error);
        }
        first.map_or(Ok(()), Err)
    }
}
impl Drop for WebGl {
    fn drop(&mut self) {
        if self.native.make_current().is_ok() {
            if self.vertex_arrays.default_native != 0 {
                unsafe {
                    extensions::delete_vertex_array(
                        self.vertex_arrays.default_native,
                        self.options.api,
                    );
                }
            }
            self.objects.delete_all();
            self.surface.destroy();
        }
        self.native.destroy();
    }
}
