//! Opaque fences keep native pointers on the owner and publish only between tasks.
use super::sync_entries::{Entries, Handle};
use super::{ApiVersion, Command, MAX_OBJECTS, Result, WebGl, gl};
use serde_json::{Value, json};
use std::collections::HashMap;

const STATUS: u32 = 0x9114;
const SIGNALED: u32 = 0x9119;
const UNSIGNALED: u32 = 0x9118;
const CONDITION: u32 = 0x9117;
const TIMEOUT: u32 = 0x911b;
const WAIT_FAILED: u32 = 0x911d;

#[derive(Default)]
pub(super) struct State {
    records: HashMap<u32, Record>,
}
struct Record {
    native: Handle,
    status: u32,
}
impl State {
    pub(super) fn destroy(&mut self, entries: &Entries) {
        // The enclosing WebGl calls this BEFORE destroying its current EGL
        // context. Field drop order must never destroy pointer-valued resources.
        for (_, record) in self.records.drain() {
            unsafe { (entries.delete)(record.native) };
        }
    }
}

impl WebGl {
    pub(super) fn sync_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        if c.op == "fenceSync" {
            if c.u(0)? != CONDITION {
                return Err(gl::INVALID_ENUM);
            }
            if c.u(1)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
            if self.sync_objects.records.len() >= MAX_OBJECTS {
                return Err(gl::OUT_OF_MEMORY);
            }
            let id = super::objects::next_browser_name()?;
            let native = unsafe { (self.core.as_ref().unwrap().sync.fence)(CONDITION, 0) };
            self.driver_result()?;
            if native.is_null() {
                return Err(gl::OUT_OF_MEMORY);
            }
            self.sync_objects.records.insert(
                id,
                Record {
                    native,
                    status: UNSIGNALED,
                },
            );
            return Ok(json!(id));
        }
        let id = c.u(0)?;
        if c.op == "isSync" {
            return Ok(json!(self.sync_objects.records.contains_key(&id)));
        }
        if c.op == "deleteSync" && id == 0 {
            return Ok(Value::Null);
        }
        let record = self
            .sync_objects
            .records
            .get(&id)
            .ok_or(gl::INVALID_OPERATION)?;
        let native = record.native;
        let status = record.status;
        match c.op.as_str() {
            "deleteSync" => {
                unsafe { (self.core.as_ref().unwrap().sync.delete)(native) };
                self.driver_result()?;
                self.sync_objects.records.remove(&id);
                Ok(Value::Null)
            }
            "clientWaitSync" => {
                let flags = c.u(1)?;
                // The admitted maximum is zero; do not block the renderer owner
                // or other contexts behind a user-specified GPU wait.
                if flags & !1 != 0 || c.i.get(2) != Some(&0) {
                    return Err(gl::INVALID_OPERATION);
                }
                if status == UNSIGNALED {
                    if flags == 1 {
                        unsafe { gl::Flush() };
                    }
                    self.driver_result()?;
                    return Ok(json!(TIMEOUT));
                }
                let result =
                    unsafe { (self.core.as_ref().unwrap().sync.client_wait)(native, flags, 0) };
                self.driver_result()?;
                Ok(json!(result))
            }
            "waitSync" => {
                if c.u(1)? != 0 || c.i.get(2) != Some(&-1) {
                    return Err(gl::INVALID_VALUE);
                }
                // TIMEOUT_IGNORED is all 64 bits, never a rounded JS Number.
                unsafe { (self.core.as_ref().unwrap().sync.server_wait)(native, 0, u64::MAX) };
                self.driver_result()?;
                Ok(Value::Null)
            }
            "getSyncParameter" => {
                let pname = c.u(1)?;
                if ![0x9112, 0x9113, STATUS, 0x9115].contains(&pname) {
                    return Err(gl::INVALID_ENUM);
                }
                if pname == STATUS {
                    return Ok(json!(status));
                }
                let mut length = 0;
                let mut value = 0;
                unsafe {
                    (self.core.as_ref().unwrap().sync.query)(
                        native,
                        pname,
                        1,
                        &mut length,
                        &mut value,
                    )
                };
                self.driver_result()?;
                if length != 1 {
                    return Err(gl::INVALID_OPERATION);
                }
                Ok(json!(value))
            }
            _ => Err(gl::INVALID_OPERATION),
        }
    }
    pub(super) fn publish_sync_results(&mut self) -> Result<()> {
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .sync
            .client_wait;
        for record in self.sync_objects.records.values_mut() {
            if record.status == SIGNALED {
                continue;
            }
            // A zero-time native wait supplies real completion without blocking.
            // Availability is sampled once at a trusted task boundary, then frozen.
            let result = unsafe { function(record.native, 0, 0) };
            match result {
                0x911a | 0x911c => record.status = SIGNALED,
                TIMEOUT => {}
                WAIT_FAILED => return Err(gl::INVALID_OPERATION),
                _ => return Err(gl::INVALID_OPERATION),
            }
        }
        self.driver_result()
    }
}
