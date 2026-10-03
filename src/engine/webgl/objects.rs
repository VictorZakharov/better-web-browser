//! Opaque, monotonically allocated names. A browser name is never a driver name.
use super::{MAX_OBJECTS, Result, gl};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_OBJECT: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Buffer,
    Shader,
    Program,
    Uniform,
    Texture,
    Framebuffer,
    Renderbuffer,
    VertexArray,
}
pub(super) struct Object {
    pub kind: Kind,
    pub native: u32,
    pub bytes: Vec<u8>,
    pub capacity: usize,
    pub buffer_target: u32,
    pub owner: u32,
    pub generation: u32,
    pub pending_delete: bool,
    pub shader_log: String,
    pub uniform_type: u32,
    pub texture_images: HashMap<(u32, i32), (u32, u32)>,
    references: u32,
    attached: Vec<u32>,
}
#[derive(Default)]
pub(super) struct Objects {
    entries: HashMap<u32, Object>,
}
impl Objects {
    pub fn uniform(&self, owner: u32, generation: u32, native: u32) -> Option<u32> {
        self.entries.iter().find_map(|(&id, object)| {
            (object.kind == Kind::Uniform
                && object.owner == owner
                && object.generation == generation
                && object.native == native)
                .then_some(id)
        })
    }
    pub fn insert(&mut self, kind: Kind, native: u32) -> Result<u32> {
        if (native == 0 && kind != Kind::Uniform) || self.entries.len() >= MAX_OBJECTS {
            return Err(gl::OUT_OF_MEMORY);
        }
        // Names must not alias in peer contexts or after a context is restored.
        let id = NEXT_OBJECT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| gl::OUT_OF_MEMORY)?;
        self.entries.insert(
            id,
            Object {
                kind,
                native,
                bytes: Vec::new(),
                capacity: 0,
                buffer_target: 0,
                owner: 0,
                generation: 0,
                pending_delete: false,
                shader_log: String::new(),
                uniform_type: 0,
                texture_images: HashMap::new(),
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
            Ok(self.get(id, kind)?.native)
        }
    }
    pub fn delete(&mut self, id: u32, kind: Kind) -> Result<()> {
        if id == 0 {
            return Ok(());
        }
        self.get(id, kind)?;
        if matches!(kind, Kind::Shader | Kind::Program | Kind::Buffer) {
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
            destroy(object);
        }
        Ok(())
    }
    pub fn delete_all(&mut self) {
        for (_, object) in self.entries.drain() {
            destroy(object);
        }
    }
    pub fn attach(&mut self, program: u32, shader: u32) -> Result<()> {
        self.get_mut(program, Kind::Program)?.attached.push(shader);
        let object = self.get_mut(shader, Kind::Shader)?;
        object.references = object.references.checked_add(1).ok_or(gl::OUT_OF_MEMORY)?;
        Ok(())
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
    fn release(&mut self, id: u32) {
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
            // GLES already received Delete*. Names are held only while attached/current;
            // once the last reference disappears they must never alias a recycled driver ID.
            if object.kind == Kind::Buffer {
                destroy(object);
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
}
fn destroy(object: Object) {
    // SAFETY: called only with the owning context current and a typed live driver name.
    unsafe {
        match object.kind {
            Kind::Buffer => gl::DeleteBuffers(1, &object.native),
            Kind::Shader if !object.pending_delete => gl::DeleteShader(object.native),
            Kind::Program if !object.pending_delete => gl::DeleteProgram(object.native),
            Kind::Shader | Kind::Program => {}
            Kind::Uniform => {}
            Kind::Texture => gl::DeleteTextures(1, &object.native),
            Kind::Framebuffer => gl::DeleteFramebuffers(1, &object.native),
            Kind::Renderbuffer => gl::DeleteRenderbuffers(1, &object.native),
            Kind::VertexArray => super::extensions::delete_vertex_array(object.native),
        }
    }
}
