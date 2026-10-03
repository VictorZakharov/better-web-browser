//! Browser-owned vertex-array state and deferred buffer storage retirement.
//! https://registry.khronos.org/webgl/extensions/OES_vertex_array_object/
use super::{Command, Kind, Result, WebGl, buffers::Attribute, gl, json};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone)]
struct State {
    attributes: Vec<Attribute>,
    element: u32,
}
#[derive(Default)]
pub(super) struct VertexArrays {
    pub(super) bound: u32,
    pub(super) default_native: u32,
    states: HashMap<u32, State>,
}
impl WebGl {
    pub(super) fn enable_vertex_arrays(&mut self) -> Result<bool> {
        if self.vertex_arrays.default_native != 0 {
            return Ok(true);
        }
        if !self.extensions.enable_vertex_arrays() {
            return Ok(false);
        }
        let native = self.rebuild_vertex_array()?;
        self.vertex_arrays.default_native = native;
        Ok(true)
    }
    fn current_vertex_state(&self) -> State {
        State {
            attributes: self.attributes.clone(),
            element: self.element_buffer,
        }
    }
    fn array_native(&self, id: u32) -> Result<u32> {
        if id == 0 {
            Ok(self.vertex_arrays.default_native)
        } else {
            self.objects.name(id, Kind::VertexArray)
        }
    }
    pub(super) fn vertex_array_command(&mut self, c: &Command) -> Result<Value> {
        if self.vertex_arrays.default_native == 0 {
            return Err(gl::INVALID_OPERATION);
        }
        match c.op.as_str() {
            "createVertexArrayOES" => {
                let mut native = 0;
                let generate = self.extensions.gen_arrays.ok_or(gl::INVALID_OPERATION)?;
                unsafe {
                    generate(1, &mut native);
                }
                self.driver_result()?;
                let id = match self.objects.insert(Kind::VertexArray, native) {
                    Ok(id) => id,
                    Err(error) => {
                        unsafe {
                            super::extensions::delete_vertex_array(native);
                        }
                        return Err(error);
                    }
                };
                self.vertex_arrays.states.insert(
                    id,
                    State {
                        attributes: vec![Attribute::default(); self.attributes.len()],
                        element: 0,
                    },
                );
                Ok(json!(id))
            }
            "bindVertexArrayOES" => {
                self.bind_vertex_array(c.u(0)?)?;
                Ok(Value::Null)
            }
            "deleteVertexArrayOES" => {
                let id = c.u(0)?;
                if id == 0 {
                    return Ok(Value::Null);
                }
                if self.objects.get(id, Kind::VertexArray).is_err() {
                    return Ok(Value::Null);
                }
                if self.vertex_arrays.bound == id {
                    self.bind_vertex_array(0)?;
                }
                if let Some(state) = self.vertex_arrays.states.remove(&id) {
                    self.release_vertex_state(state)?;
                }
                self.objects.delete(id, Kind::VertexArray)?;
                self.driver_result()?;
                Ok(Value::Null)
            }
            "isVertexArrayOES" => {
                let id = c.u(0)?;
                let Some(object) = self.objects.get(id, Kind::VertexArray).ok() else {
                    return Ok(json!(false));
                };
                let predicate = self.extensions.is_array.ok_or(gl::INVALID_OPERATION)?;
                Ok(json!(unsafe { predicate(object.native) } != 0))
            }
            _ => Err(gl::INVALID_OPERATION),
        }
    }
    fn bind_vertex_array(&mut self, id: u32) -> Result<()> {
        if id == self.vertex_arrays.bound {
            return Ok(());
        }
        let native = self.array_native(id)?;
        let next = self
            .vertex_arrays
            .states
            .get(&id)
            .cloned()
            .ok_or(gl::INVALID_OPERATION)?;
        let bind = self.extensions.bind_array.ok_or(gl::INVALID_OPERATION)?;
        unsafe {
            bind(native);
        }
        self.driver_result()?;
        self.vertex_arrays
            .states
            .insert(self.vertex_arrays.bound, self.current_vertex_state());
        self.vertex_arrays.bound = id;
        self.attributes = next.attributes;
        self.element_buffer = next.element;
        Ok(())
    }
    fn release_vertex_state(&mut self, state: State) -> Result<()> {
        self.objects.switch_buffer(state.element, 0)?;
        for attribute in state.attributes {
            self.objects.switch_buffer(attribute.buffer, 0)?;
        }
        Ok(())
    }
    pub(super) fn delete_buffer(&mut self, id: u32) -> Result<Value> {
        if id == 0 {
            return Ok(Value::Null);
        }
        if self.objects.get(id, Kind::Buffer)?.pending_delete {
            return Ok(Value::Null);
        }
        // Mark deletion only after detaching current-array references. Inactive
        // arrays keep both driver storage and the matching CPU validation mirror.
        self.detach_core_buffer(id);
        if self.array_buffer == id {
            self.array_buffer = 0;
            unsafe {
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            }
        }
        if self.element_buffer == id {
            self.objects.switch_buffer(id, 0)?;
            self.element_buffer = 0;
            unsafe {
                gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            }
        }
        for index in 0..self.attributes.len() {
            if self.attributes[index].buffer == id {
                self.attributes[index].buffer = 0;
                self.objects.switch_buffer(id, 0)?;
            }
        }
        if self.vertex_arrays.default_native != 0 {
            // GLES2 cannot set an attribute's buffer binding to zero without
            // client arrays. Rebuild this array from checked browser state;
            // this avoids prematurely deleting storage still used by peers.
            let old = self.array_native(self.vertex_arrays.bound)?;
            let replacement = self.rebuild_vertex_array()?;
            if self.vertex_arrays.bound == 0 {
                self.vertex_arrays.default_native = replacement;
            } else {
                self.objects.replace_native(
                    self.vertex_arrays.bound,
                    Kind::VertexArray,
                    replacement,
                )?;
            }
            unsafe {
                super::extensions::delete_vertex_array(old);
            }
        }
        self.objects.delete(id, Kind::Buffer)?;
        self.driver_result()?;
        Ok(Value::Null)
    }
    fn rebuild_vertex_array(&mut self) -> Result<u32> {
        let generate = self.extensions.gen_arrays.ok_or(gl::INVALID_OPERATION)?;
        let bind = self.extensions.bind_array.ok_or(gl::INVALID_OPERATION)?;
        let old = self.array_native(self.vertex_arrays.bound)?;
        let mut native = 0;
        unsafe {
            generate(1, &mut native);
            bind(native);
        }
        let result = (|| {
            for (index, attribute) in self.attributes.iter().enumerate() {
                if attribute.buffer != 0 {
                    let buffer = self.objects.name(attribute.buffer, Kind::Buffer)?;
                    let component = match attribute.kind {
                        gl::BYTE | gl::UNSIGNED_BYTE => 1,
                        gl::SHORT | gl::UNSIGNED_SHORT => 2,
                        _ => 4,
                    };
                    unsafe {
                        gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
                        gl::VertexAttribPointer(
                            index as u32,
                            (attribute.size / component) as i32,
                            attribute.kind,
                            u8::from(attribute.normalized),
                            attribute.stride as i32,
                            attribute.offset as *const _,
                        );
                    }
                }
                if attribute.enabled {
                    unsafe {
                        gl::EnableVertexAttribArray(index as u32);
                    }
                }
                if self.extensions.instancing {
                    let divisor = self.extensions.divisor.ok_or(gl::INVALID_OPERATION)?;
                    unsafe {
                        divisor(index as u32, attribute.divisor);
                    }
                }
            }
            unsafe {
                gl::BindBuffer(
                    gl::ELEMENT_ARRAY_BUFFER,
                    self.objects.name(self.element_buffer, Kind::Buffer)?,
                );
            }
            self.driver_result()
        })();
        unsafe {
            gl::BindBuffer(
                gl::ARRAY_BUFFER,
                self.objects.name(self.array_buffer, Kind::Buffer)?,
            );
        }
        if let Err(error) = result {
            unsafe {
                bind(old);
                super::extensions::delete_vertex_array(native);
            }
            return Err(error);
        }
        // The default array is private, so enabling the extension preserves
        // pre-extension bindings rather than exposing the driver's bare name 0.
        if self.vertex_arrays.default_native == 0 {
            self.vertex_arrays
                .states
                .insert(0, self.current_vertex_state());
        }
        Ok(native)
    }
}
