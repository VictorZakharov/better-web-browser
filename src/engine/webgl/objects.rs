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
}
pub(super) struct Object {
    pub kind: Kind,
    pub native: u32,
    pub bytes: Vec<u8>,
    pub capacity: usize,
    pub buffer_target: u32,
    pub owner: u32,
    pub generation: u32,
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
}
fn destroy(object: Object) {
    // SAFETY: called only with the owning context current and a typed live driver name.
    unsafe {
        match object.kind {
            Kind::Buffer => gl::DeleteBuffers(1, &object.native),
            Kind::Shader => gl::DeleteShader(object.native),
            Kind::Program => gl::DeleteProgram(object.native),
            Kind::Uniform => {}
        }
    }
}
