//! Occlusion/primitive counters are native; publication is cached between tasks.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
use std::collections::HashMap;

const ANY: u32 = 0x8c2f;
const CONSERVATIVE: u32 = 0x8d6a;
const PRIMITIVES: u32 = 0x8c88;
const CURRENT: u32 = 0x8865;
const RESULT: u32 = 0x8866;
const AVAILABLE: u32 = 0x8867;

#[derive(Default)]
pub(super) struct State {
    records: HashMap<u32, Record>,
    active: [u32; 2],
}
#[derive(Default)]
struct Record {
    target: u32,
    ended: bool,
    available: bool,
    result: u32,
}
fn slot(target: u32) -> Result<usize> {
    match target {
        ANY | CONSERVATIVE => Ok(0),
        PRIMITIVES => Ok(1),
        _ => Err(gl::INVALID_ENUM),
    }
}

pub(super) unsafe fn delete_native(name: u32) {
    // SAFETY: compile-time GLES3 symbol, exact pinned Khronos ABI, current owner.
    let pointer = unsafe { mozangle::egl::ffi::GetProcAddress(c"glDeleteQueries".as_ptr()) };
    let function = unsafe {
        std::mem::transmute::<
            mozangle::egl::ffi::types::__eglMustCastToProperFunctionPointerType,
            super::extensions::DeleteArrays,
        >(pointer)
    };
    unsafe { function(1, &name) };
}

impl WebGl {
    pub(super) fn query_object_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        if c.op == "completeGpuTask" {
            // Internal foundation protocol only. No public realm binding forwards
            // this opcode. Admission needs a trusted host event-loop completion
            // hook; presentation/flush/finish are NOT task-completion substitutes.
            self.publish_sync_results()?;
            return self.publish_query_results();
        }
        if c.op == "createQuery" {
            let mut native = 0;
            unsafe { (self.core.as_ref().unwrap().gen_queries)(1, &mut native) };
            self.driver_result()?;
            let id = match self.objects.insert(Kind::Query, native) {
                Ok(id) => id,
                Err(error) => {
                    unsafe { delete_native(native) };
                    return Err(error);
                }
            };
            self.query_objects.records.insert(id, Record::default());
            return Ok(json!(id));
        }
        if c.op == "isQuery" {
            // glGenQueries reserves a name; a query object exists after first begin.
            return Ok(json!(
                self.query_objects
                    .records
                    .get(&c.u(0)?)
                    .is_some_and(|record| record.target != 0)
            ));
        }
        if matches!(c.op.as_str(), "beginQuery" | "endQuery" | "getQuery") {
            let target = c.u(0)?;
            let slot = slot(target)?;
            let active = self.query_objects.active[slot];
            if c.op == "getQuery" {
                if c.u(1)? != CURRENT {
                    return Err(gl::INVALID_ENUM);
                }
                return Ok(
                    if active != 0 && self.query_objects.records[&active].target == target {
                        json!(active)
                    } else {
                        Value::Null
                    },
                );
            }
            if c.op == "beginQuery" {
                let id = c.u(1)?;
                let native = self.objects.get(id, Kind::Query)?.native;
                let record = &self.query_objects.records[&id];
                if active != 0 || (record.target != 0 && record.target != target) {
                    return Err(gl::INVALID_OPERATION);
                }
                unsafe { (self.core.as_ref().unwrap().begin_query)(target, native) };
                self.driver_result()?;
                self.query_objects.records.insert(
                    id,
                    Record {
                        target,
                        ..Record::default()
                    },
                );
                self.query_objects.active[slot] = id;
            } else {
                if active == 0 || self.query_objects.records[&active].target != target {
                    return Err(gl::INVALID_OPERATION);
                }
                unsafe { (self.core.as_ref().unwrap().end_query)(target) };
                self.driver_result()?;
                self.query_objects.active[slot] = 0;
                self.query_objects.records.get_mut(&active).unwrap().ended = true;
            }
            return Ok(Value::Null);
        }
        let id = c.u(0)?;
        if c.op == "deleteQuery" && id == 0 {
            return Ok(Value::Null);
        }
        self.objects.get(id, Kind::Query)?;
        if c.op == "deleteQuery" {
            if let Some(slot) = self
                .query_objects
                .active
                .iter()
                .position(|&active| active == id)
            {
                let target = self.query_objects.records[&id].target;
                unsafe { (self.core.as_ref().unwrap().end_query)(target) };
                self.driver_result()?;
                self.query_objects.active[slot] = 0;
            }
            self.objects.delete(id, Kind::Query)?;
            self.query_objects.records.remove(&id);
            self.driver_result()?;
            return Ok(Value::Null);
        }
        let record = &self.query_objects.records[&id];
        if !record.ended {
            return Err(gl::INVALID_OPERATION);
        }
        match c.u(1)? {
            RESULT => Ok(json!(record.result)),
            AVAILABLE => Ok(json!(record.available)),
            _ => Err(gl::INVALID_ENUM),
        }
    }
    fn publish_query_results(&mut self) -> Result<Value> {
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .query_result;
        for (&id, record) in &mut self.query_objects.records {
            if !record.ended || record.available {
                continue;
            }
            let native = self.objects.get(id, Kind::Query)?.native;
            let mut available = 0;
            unsafe { function(native, AVAILABLE, &mut available) };
            if available != 0 {
                // Querying RESULT can block unless AVAILABLE has become true.
                // Never poll it from an author's same-task busy loop.
                unsafe { function(native, RESULT, &mut record.result) };
                record.available = true;
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
