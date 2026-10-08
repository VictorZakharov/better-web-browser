//! Opaque, monotonically allocated names. A browser name is never a driver name.
use super::{MAX_OBJECTS, Result, gl};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_OBJECT: AtomicU32 = AtomicU32::new(1);

mod storage_summary;
mod uniform_locations;
use uniform_locations::{UniformLocation, UniformLocations};

pub(super) fn next_browser_name() -> Result<u32> {
    NEXT_OBJECT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| gl::OUT_OF_MEMORY)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Buffer,
    Shader,
    Program,
    Texture,
    Framebuffer,
    Renderbuffer,
    VertexArray,
    Sampler,
    Query,
    TransformFeedback,
}
pub(super) struct Object {
    pub kind: Kind,
    pub native: u32,
    pub bytes: Vec<u8>,
    // Only initialized CPU uploads/copies are authoritative. GPU capture and
    // pixel-pack writes invalidate this mirror until a full native readback.
    pub buffer_mirror_valid: bool,
    pub capacity: usize,
    pub buffer_target: u32,
    pub generation: u32,
    pub pending_delete: bool,
    // GLES3 detaches current bindings. An uninstantiated reservation prevents
    // ANGLE from recycling the numeric name while inactive containers retain it.
    pub native_deleted: bool,
    pub shader_log: String,
    pub texture_images: HashMap<(u32, i32), (u32, u32)>,
    pub texture_allocations: HashMap<(u32, i32), usize>,
    pub core_images: HashMap<(u32, i32), super::core_textures::Image>,
    pub immutable_levels: u32,
    pub renderbuffer_format: u32,
    pub framebuffer_attachments: HashMap<u32, super::framebuffer_attachments::Attachment>,
    pub draw_buffers: Vec<u32>,
    pub read_buffer: u32,
    references: u32,
    attached: Vec<u32>,
}
#[derive(Default)]
pub(super) struct Objects {
    pub(super) poisoned: bool,
    api: super::ApiVersion,
    entries: HashMap<u32, Object>,
    uniforms: UniformLocations,
    retired_capacity: usize,
}
impl Objects {
    pub(super) fn new(api: super::ApiVersion) -> Self {
        Self {
            poisoned: false,
            api,
            entries: HashMap::new(),
            uniforms: UniformLocations::default(),
            retired_capacity: 0,
        }
    }
    pub fn uniform(&self, owner: u32, generation: u32, native: u32) -> Option<u32> {
        self.uniforms.find(owner, generation, native)
    }
    pub fn insert_uniform(&mut self, owner: u32, native: u32, uniform_type: u32) -> Result<u32> {
        let generation = self.get(owner, Kind::Program)?.generation;
        self.uniforms.insert(UniformLocation {
            native,
            owner,
            generation,
            uniform_type,
        })
    }
    pub fn uniform_location(&self, id: u32) -> Result<&UniformLocation> {
        self.uniforms.get(id)
    }
    pub fn begin_link(&mut self, id: u32) -> Result<u32> {
        let object = self.get_mut(id, Kind::Program)?;
        object.generation = object.generation.checked_add(1).ok_or(gl::OUT_OF_MEMORY)?;
        let native = object.native;
        // WebGL locations expire on every relink, even when the link fails.
        self.uniforms.retire(id);
        Ok(native)
    }
    pub fn insert(&mut self, kind: Kind, native: u32) -> Result<u32> {
        if native == 0 || self.entries.len() >= MAX_OBJECTS {
            return Err(gl::OUT_OF_MEMORY);
        }
        // Names must not alias in peer contexts or after a context is restored.
        let id = next_browser_name()?;
        self.entries.insert(
            id,
            Object {
                kind,
                native,
                bytes: Vec::new(),
                buffer_mirror_valid: true,
                capacity: 0,
                buffer_target: 0,
                generation: 0,
                pending_delete: false,
                native_deleted: false,
                shader_log: String::new(),
                texture_images: HashMap::new(),
                texture_allocations: HashMap::new(),
                core_images: HashMap::new(),
                immutable_levels: 0,
                renderbuffer_format: 0,
                framebuffer_attachments: HashMap::new(),
                draw_buffers: vec![gl::COLOR_ATTACHMENT0],
                read_buffer: gl::COLOR_ATTACHMENT0,
                references: 0,
                attached: Vec::new(),
            },
        );
        Ok(id)
    }
    pub fn get(&self, id: u32, kind: Kind) -> Result<&Object> {
        self.entries
            .get(&id)
            .filter(|o| o.kind == kind)
            .ok_or(gl::INVALID_OPERATION)
    }
    pub fn get_mut(&mut self, id: u32, kind: Kind) -> Result<&mut Object> {
        self.entries
            .get_mut(&id)
            .filter(|o| o.kind == kind)
            .ok_or(gl::INVALID_OPERATION)
    }
    pub fn name(&self, id: u32, kind: Kind) -> Result<u32> {
        if id == 0 {
            Ok(0)
        } else {
            let object = self.get(id, kind)?;
            if object.native_deleted {
                Err(gl::INVALID_OPERATION)
            } else {
                Ok(object.native)
            }
        }
    }
    pub(super) fn delete_native_buffer_name(&mut self, id: u32) -> Result<()> {
        let object = self.get_mut(id, Kind::Buffer)?;
        if !object.native_deleted {
            unsafe {
                gl::DeleteBuffers(1, &object.native);
            }
            object.native_deleted = true;
            if let Err(error) = super::buffer_retirement::reserve_deleted_name(object.native) {
                // A provider allocation failure after deletion cannot be rolled
                // back. Lose the owner context rather than permit ID aliasing.
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn delete(&mut self, id: u32, kind: Kind) -> Result<()> {
        if id == 0 {
            return Ok(());
        }
        self.get(id, kind)?;
        if matches!(
            kind,
            Kind::Shader | Kind::Program | Kind::Buffer | Kind::Texture | Kind::Renderbuffer
        ) {
            let object = self.get_mut(id, kind)?;
            if object.pending_delete {
                return Ok(());
            }
            object.pending_delete = true;
            unsafe {
                if kind == Kind::Shader {
                    gl::DeleteShader(object.native);
                } else if kind == Kind::Program {
                    gl::DeleteProgram(object.native);
                }
            }
            self.retire_unreferenced(id);
            return Ok(());
        }
        if let Some(object) = self.entries.remove(&id) {
            self.retired_capacity += object.capacity;
            let attached: Vec<_> = object
                .framebuffer_attachments
                .values()
                .map(|entry| entry.id)
                .collect();
            destroy(object, self.api);
            for resource in attached {
                self.release(resource);
            }
        }
        Ok(())
    }
    pub fn delete_all(&mut self) {
        self.uniforms.clear();
        for (_, object) in self.entries.drain() {
            self.retired_capacity += object.capacity;
            destroy(object, self.api);
        }
    }
    pub fn attach(&mut self, program: u32, shader: u32) -> Result<()> {
        self.get_mut(program, Kind::Program)?.attached.push(shader);
        let object = self.get_mut(shader, Kind::Shader)?;
        object.references = object.references.checked_add(1).ok_or(gl::OUT_OF_MEMORY)?;
        Ok(())
    }
    pub(super) fn attached_shaders(&self, program: u32) -> Result<&[u32]> {
        Ok(&self.get(program, Kind::Program)?.attached)
    }
    pub fn detach(&mut self, program: u32, shader: u32) -> Result<()> {
        self.get_mut(program, Kind::Program)?
            .attached
            .retain(|id| *id != shader);
        self.release(shader);
        Ok(())
    }
    pub fn switch_program(&mut self, old: u32, new: u32) -> Result<()> {
        if old == new {
            return Ok(());
        }
        if new != 0 {
            self.get_mut(new, Kind::Program)?.references += 1;
        }
        if old != 0 {
            self.release(old);
        }
        Ok(())
    }
    pub fn switch_buffer(&mut self, old: u32, new: u32) -> Result<()> {
        if old == new {
            return Ok(());
        }
        if new != 0 {
            self.get_mut(new, Kind::Buffer)?.references += 1;
        }
        if old != 0 {
            self.release(old);
        }
        Ok(())
    }
    pub fn replace_native(&mut self, id: u32, kind: Kind, native: u32) -> Result<()> {
        self.get_mut(id, kind)?.native = native;
        Ok(())
    }
    pub(super) fn retain(&mut self, id: u32, kind: Kind) -> Result<()> {
        let object = self.get_mut(id, kind)?;
        object.references = object.references.checked_add(1).ok_or(gl::OUT_OF_MEMORY)?;
        Ok(())
    }
    pub(super) fn release(&mut self, id: u32) {
        if let Some(object) = self.entries.get_mut(&id) {
            object.references = object.references.saturating_sub(1);
        }
        self.retire_unreferenced(id);
    }
    fn retire_unreferenced(&mut self, id: u32) {
        if self
            .entries
            .get(&id)
            .is_some_and(|o| o.pending_delete && o.references == 0)
            && let Some(object) = self.entries.remove(&id)
        {
            self.retired_capacity += object.capacity;
            if object.kind == Kind::Program {
                // A deleted current program remains alive until it is unbound.
                self.uniforms.retire(id);
            }
            // GLES already received Delete*. Names are held only while attached/current;
            // once the last reference disappears they must never alias a recycled driver ID.
            if matches!(
                object.kind,
                Kind::Buffer | Kind::Texture | Kind::Renderbuffer
            ) {
                destroy(object, self.api);
                return;
            }
            for shader in object.attached {
                self.release(shader);
            }
        }
    }
    pub fn public_name(&self, native: u32, kind: Kind) -> Option<u32> {
        self.entries.iter().find_map(|(&id, object)| {
            (object.kind == kind && object.native == native).then_some(id)
        })
    }
    pub(super) fn take_retired_capacity(&mut self) -> usize {
        std::mem::take(&mut self.retired_capacity)
    }
}
fn destroy(object: Object, api: super::ApiVersion) {
    // SAFETY: called only with the owning context current and a typed live driver name.
    unsafe {
        match object.kind {
            // For native_deleted this releases only a generated, never-bound
            // reservation; ANGLE therefore performs no second detachment.
            Kind::Buffer => gl::DeleteBuffers(1, &object.native),
            Kind::Shader if !object.pending_delete => gl::DeleteShader(object.native),
            Kind::Program if !object.pending_delete => gl::DeleteProgram(object.native),
            Kind::Shader | Kind::Program => {}
            Kind::Texture => gl::DeleteTextures(1, &object.native),
            Kind::Framebuffer => gl::DeleteFramebuffers(1, &object.native),
            Kind::Renderbuffer => gl::DeleteRenderbuffers(1, &object.native),
            Kind::VertexArray => super::extensions::delete_vertex_array(object.native, api),
            Kind::Sampler => super::extensions::delete_sampler(object.native),
            Kind::Query => super::query_objects::delete_native(object.native),
            Kind::TransformFeedback => super::transform_entries::delete_native(object.native),
        }
    }
}
