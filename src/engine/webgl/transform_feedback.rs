//! Transform-feedback objects own indexed capture ranges and lifecycle state.
use super::indexed_uniform_buffers::Binding;
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) struct State {
    pub bound: u32,
    pub records: HashMap<u32, Record>,
    pub count: usize,
}
pub(super) struct Record {
    pub bindings: Vec<Binding>,
    pub active: bool,
    pub paused: bool,
    pub program: u32,
    pub ledger: super::transform_capacity::Ledger,
    seen: bool,
}
impl Record {
    fn new(count: usize) -> Self {
        Self {
            bindings: vec![Binding::default(); count],
            active: false,
            paused: false,
            seen: false,
            program: 0,
            ledger: super::transform_capacity::Ledger::default(),
        }
    }
}
impl State {
    pub(super) fn new(api: ApiVersion) -> std::result::Result<Self, String> {
        let mut count = 0;
        if api == ApiVersion::Two {
            unsafe { gl::GetIntegerv(0x8c8b, &mut count) };
            if !(4..=32).contains(&count) {
                return Err(
                    "ANGLE transform-feedback binding limit is outside admitted bounds".into(),
                );
            }
        }
        Ok(Self {
            bound: 0,
            records: HashMap::from([(0, Record::new(count as usize))]),
            count: count as usize,
        })
    }
    pub(super) fn any_active(&self) -> bool {
        self.records.values().any(|record| record.active)
    }
}
impl WebGl {
    pub(super) fn transform_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        if c.op == "createTransformFeedback" {
            let mut native = 0;
            unsafe { (self.core.as_ref().unwrap().transform.gen_objects)(1, &mut native) };
            self.driver_result()?;
            let id = match self.objects.insert(Kind::TransformFeedback, native) {
                Ok(id) => id,
                Err(error) => {
                    unsafe { super::transform_entries::delete_native(native) };
                    return Err(error);
                }
            };
            self.transform_feedback
                .records
                .insert(id, Record::new(self.transform_feedback.count));
            return Ok(json!(id));
        }
        if c.op == "isTransformFeedback" {
            return Ok(json!(
                c.u(0)? != 0
                    && self
                        .transform_feedback
                        .records
                        .get(&c.u(0)?)
                        .is_some_and(|record| record.seen)
            ));
        }
        if c.op == "bindTransformFeedback" {
            if c.u(0)? != 0x8e22 {
                return Err(gl::INVALID_ENUM);
            }
            let id = c.u(1)?;
            let native = self.objects.name(id, Kind::TransformFeedback)?;
            let record = &self.transform_feedback.records[&self.transform_feedback.bound];
            if record.active && !record.paused {
                return Err(gl::INVALID_OPERATION);
            }
            unsafe { (self.core.as_ref().unwrap().transform.bind)(0x8e22, native) };
            self.driver_result()?;
            self.transform_feedback.bound = id;
            self.transform_feedback.records.get_mut(&id).unwrap().seen = true;
            return Ok(Value::Null);
        }
        if c.op == "deleteTransformFeedback" {
            let id = c.u(0)?;
            if id == 0 {
                return Ok(Value::Null);
            }
            self.objects.get(id, Kind::TransformFeedback)?;
            if self.transform_feedback.records[&id].active {
                return Err(gl::INVALID_OPERATION);
            }
            if self.transform_feedback.bound == id {
                unsafe { (self.core.as_ref().unwrap().transform.bind)(0x8e22, 0) };
                self.driver_result()?;
                self.transform_feedback.bound = 0;
            }
            self.objects.delete(id, Kind::TransformFeedback)?;
            let record = self.transform_feedback.records.remove(&id).unwrap();
            for binding in record.bindings {
                self.objects.release(binding.id);
            }
            self.driver_result()?;
            return Ok(Value::Null);
        }
        let bound = self.transform_feedback.bound;
        let record = &self.transform_feedback.records[&bound];
        let entries = &self.core.as_ref().unwrap().transform;
        let mut ledger = None;
        match c.op.as_str() {
            "beginTransformFeedback" => {
                let mode = c.u(0)?;
                if ![gl::POINTS, gl::LINES, gl::TRIANGLES].contains(&mode) {
                    return Err(gl::INVALID_ENUM);
                }
                if record.active {
                    return Err(gl::INVALID_OPERATION);
                }
                self.validate_program()?;
                let mut unique = std::collections::HashSet::new();
                if record
                    .bindings
                    .iter()
                    .any(|binding| binding.id != 0 && !unique.insert(binding.id))
                {
                    return Err(gl::INVALID_OPERATION);
                }
                ledger = Some(self.new_transform_ledger(mode)?);
                unsafe { (self.core.as_ref().unwrap().transform.begin)(mode) };
            }
            "endTransformFeedback" => {
                if !record.active {
                    return Err(gl::INVALID_OPERATION);
                }
                unsafe { (entries.end)() };
            }
            "pauseTransformFeedback" => {
                if !record.active || record.paused {
                    return Err(gl::INVALID_OPERATION);
                }
                unsafe { (entries.pause)() };
            }
            "resumeTransformFeedback" => {
                if !record.active || !record.paused {
                    return Err(gl::INVALID_OPERATION);
                }
                if record.program != self.program {
                    return Err(gl::INVALID_OPERATION);
                }
                unsafe { (entries.resume)() };
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        let old_program = self.transform_feedback.records[&bound].program;
        if matches!(
            c.op.as_str(),
            "beginTransformFeedback" | "resumeTransformFeedback"
        ) {
            // Capture can overwrite any bound range. Conservatively invalidate
            // the whole mirror before draws; paused uploads can become valid
            // again, so resume must invalidate too. No synchronous GPU map here.
            for binding in &self.transform_feedback.records[&bound].bindings {
                if binding.id != 0 {
                    self.objects
                        .get_mut(binding.id, Kind::Buffer)?
                        .buffer_mirror_valid = false;
                }
            }
        }
        if c.op == "beginTransformFeedback" {
            self.objects.retain(self.program, Kind::Program)?;
        } else if c.op == "endTransformFeedback" {
            self.objects.release(old_program);
        }
        let record = self.transform_feedback.records.get_mut(&bound).unwrap();
        match c.op.as_str() {
            "beginTransformFeedback" => {
                record.active = true;
                record.program = self.program;
                record.ledger = ledger.expect("validated capture capacity");
            }
            "endTransformFeedback" => {
                record.active = false;
                record.paused = false;
                record.program = 0;
                record.ledger = super::transform_capacity::Ledger::default();
            }
            "pauseTransformFeedback" => record.paused = true,
            "resumeTransformFeedback" => record.paused = false,
            _ => unreachable!(),
        }
        Ok(Value::Null)
    }
}
