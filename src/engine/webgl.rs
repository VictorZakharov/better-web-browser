//! Renderer-owned GLES2 contexts with a WebGL compatibility/zero-initialization boundary.
//! ANGLE owns GLSL parsing, translation and driver calls. Browser handles never expose pointers.
use mozangle::gles::ffi as gl;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

mod api_version;
#[cfg(test)]
mod api_version_tests;
#[cfg(test)]
mod array_copy_boundary_tests;
mod buffer_retirement;
#[cfg(test)]
mod buffer_retirement_tests;
mod buffers;
mod commands;
#[cfg(test)]
mod compressed_buffer_tests;
mod compressed_capabilities;
#[cfg(test)]
mod compressed_core_tests;
#[cfg(test)]
mod compressed_core_validation_tests;
mod compressed_formats;
mod compressed_initialization;
mod compressed_storage;
#[cfg(test)]
mod compressed_texture_tests;
mod compressed_textures;
#[cfg(test)]
mod compressed_validation_tests;
mod compressed_volumes;
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
mod core_color_admission_tests;
#[cfg(test)]
mod core_draw_tests;
mod core_draws;
mod core_entries;
#[cfg(test)]
mod core_extension_tests;
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
mod default_attachment_queries;
#[cfg(test)]
mod default_attachment_tests;
mod depth_textures;
mod draw_buffers;
mod drawing_buffer_extent;
#[cfg(test)]
mod drawing_buffer_extent_tests;
mod extension_commands;
mod extensions;
mod float_values;
mod framebuffer_attachments;
mod framebuffer_completeness;
mod framebuffer_guard;
#[cfg(test)]
mod framebuffer_guard_tests;
mod framebuffer_invalidation;
#[cfg(test)]
mod framebuffer_invalidation_pixel_tests;
#[cfg(test)]
mod framebuffer_invalidation_storage_tests;
#[cfg(test)]
mod framebuffer_invalidation_tests;
mod framebuffer_queries;
mod framebuffers;
mod image_uploads;
mod index_ranges;
#[cfg(test)]
mod indexed_range_admission_tests;
mod indexed_uniform_buffers;
mod instancing;
mod legacy_mip_allocations;
mod multisample;
#[cfg(test)]
mod multisample_tests;
mod object_deletion;
mod object_queries;
mod objects;
mod parameters;
mod pixel_buffer_guard;
#[cfg(test)]
mod pixel_buffer_tests;
mod pixel_buffers;
mod pixel_layout;
#[cfg(test)]
mod pixel_layout_tests;
mod pixel_readback;
mod pixel_store_guard;
#[cfg(test)]
mod pixel_transfer_tests;
mod pixel_transport;
mod presentation;
mod queries;
#[cfg(test)]
mod query_object_tests;
mod query_objects;
#[cfg(test)]
mod read_pair_validation_tests;
mod readback_cache;
#[cfg(test)]
mod readback_cache_tests;
mod resize;
#[cfg(test)]
mod same_size_resize_tests;
#[cfg(test)]
mod sampler_tests;
mod samplers;
mod session;
mod shader_commands;
mod shader_queries;
mod shader_validation;
mod stencil_masks;
mod surface;
mod surface_multisample;
#[cfg(test)]
mod surface_multisample_tests;
mod sync_entries;
#[cfg(test)]
mod sync_object_tests;
mod sync_objects;
mod sync_reply_cache;
mod task_completion;
#[cfg(test)]
mod task_completion_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod texture_allocation_tests;
mod texture_allocations;
mod texture_capabilities;
mod texture_color_space;
mod texture_dispatch;
mod texture_formats;
mod texture_sampling;
mod texture_targets;
mod textures;
#[cfg(test)]
mod transform_array_tests;
mod transform_buffers;
#[cfg(test)]
mod transform_builtin_tests;
#[cfg(test)]
mod transform_delete_tests;
mod transform_entries;
mod transform_feedback;
#[cfg(test)]
mod transform_feedback_tests;
mod transform_safety;
#[cfg(test)]
mod transform_validation_tests;
mod transform_varyings;
#[cfg(test)]
mod typed_framebuffer_tests;
mod typed_framebuffers;
mod uniform_block_queries;
#[cfg(test)]
mod uniform_block_tests;
#[cfg(test)]
mod uniform_block_validation_tests;
mod uniform_blocks;
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
#[cfg(test)]
mod volume_blit_tests;
mod volume_copy;
mod volume_copy_blit;
mod volume_copy_conversion;
#[cfg(test)]
mod volume_copy_conversion_tests;
mod volume_copy_region;
#[cfg(test)]
mod volume_copy_tests;
#[cfg(test)]
mod volume_copy_transfer_tests;
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

mod backend;
use backend::BackendContexts;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Options {
    api: ApiVersion,
    alpha: bool,
    depth: bool,
    stencil: bool,
    antialias: bool,
    preserve: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            api: ApiVersion::One,
            alpha: true,
            depth: true,
            stencil: false,
            antialias: false,
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
    readback_cache: readback_cache::Cache,
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
    indexed_uniforms: indexed_uniform_buffers::Bindings,
    query_objects: query_objects::State,
    sync_objects: sync_objects::State,
    transform_feedback: transform_feedback::State,
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
