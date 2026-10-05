//! Atomic preflight of owned multi-draw lists before one genuine ANGLE call.
//! Native pointer lists contain only checked element-buffer byte offsets.
use super::{Command, MAX_DRAW_VERTICES, Result, WebGl, buffers::checked_mode, gl};
use serde_json::Value;
use std::ffi::c_void;

mod native_calls;
#[cfg(test)]
mod tests;

pub(super) const MAX_DRAWS: usize = 4096;

struct Batch {
    mode: u32,
    kind: u32,
    indexed: bool,
    instanced: bool,
    starts: Vec<i32>,
    counts: Vec<i32>,
    instances: Vec<i32>,
    work: u64,
}

impl Batch {
    fn parse(c: &Command) -> Result<Self> {
        let mode = checked_mode(c.u(0)?)?;
        let kind = c.u(1)?;
        let draws = c.u(2)? as usize;
        if draws > MAX_DRAWS {
            return Err(gl::OUT_OF_MEMORY);
        }
        if c.i.len() != 3 + draws * 3 || !c.f.is_empty() || !c.text.is_empty() {
            return Err(gl::INVALID_VALUE);
        }
        let indexed = matches!(
            c.op.as_str(),
            "multiDrawElementsWEBGL" | "multiDrawElementsInstancedWEBGL"
        );
        let instanced = matches!(
            c.op.as_str(),
            "multiDrawArraysInstancedWEBGL" | "multiDrawElementsInstancedWEBGL"
        );
        let mut batch = Self {
            mode,
            kind,
            indexed,
            instanced,
            starts: Vec::with_capacity(draws),
            counts: Vec::with_capacity(draws),
            instances: Vec::with_capacity(draws),
            work: 0,
        };
        for index in 0..draws {
            let base = 3 + index * 3;
            let start = c.n(base)?;
            let count = c.n(base + 1)?;
            let instances = c.n(base + 2)?;
            if start < 0 || count < 0 || instances < 0 || (!instanced && instances != 1) {
                return Err(gl::INVALID_VALUE);
            }
            if !indexed && i64::from(start) + i64::from(count) > i64::from(i32::MAX) + 1 {
                return Err(gl::INVALID_VALUE);
            }
            batch.work = batch
                .work
                .checked_add(count as u64 * instances as u64)
                .ok_or(gl::INVALID_VALUE)?;
            batch.starts.push(start);
            batch.counts.push(count);
            batch.instances.push(instances);
        }
        Ok(batch)
    }
}

impl WebGl {
    pub(super) fn multi_draw_command(&mut self, c: &Command) -> Result<Value> {
        if !self.extensions.multi_draw {
            return Err(gl::INVALID_OPERATION);
        }
        let batch = Batch::parse(c)?;
        if batch.work > u64::from(MAX_DRAW_VERTICES)
            || batch
                .counts
                .iter()
                .chain(&batch.instances)
                .any(|v| *v as u32 > MAX_DRAW_VERTICES)
        {
            return Err(gl::INVALID_VALUE);
        }
        let size = if batch.indexed {
            self.index_size(batch.kind)?
        } else {
            0
        };
        self.validate_program()?;
        self.validate_framebuffer()?;
        let captured = self.prepare_transform_capture(
            batch.mode,
            batch.indexed,
            batch
                .counts
                .iter()
                .zip(&batch.instances)
                .map(|(count, instances)| (*count as u32, *instances as u32)),
        )?;
        // Validate the entire batch, including late invalid draws, before any
        // driver call can change framebuffer/feedback state. Zero-work draws
        // keep their slot, preserving gl_DrawID for all following draws.
        let mut vertex_maximum = None;
        let mut instance_maximum = 0;
        for index in 0..batch.counts.len() {
            let start = batch.starts[index] as usize;
            let count = batch.counts[index] as u32;
            let instances = batch.instances[index] as u32;
            if batch.indexed && !start.is_multiple_of(size) {
                return Err(gl::INVALID_OPERATION);
            }
            if count == 0 || instances == 0 {
                continue;
            }
            let maximum = if batch.indexed {
                self.maximum_index(count as usize, size, start)?
            } else {
                Some(
                    (start as u32)
                        .checked_add(count - 1)
                        .ok_or(gl::INVALID_VALUE)?,
                )
            };
            if let Some(maximum) = maximum {
                vertex_maximum = Some(vertex_maximum.map_or(maximum, |old: u32| old.max(maximum)));
                instance_maximum = instance_maximum.max(instances);
            }
        }
        // Vertex and divisor attributes have independent maxima. Combining
        // them avoids repeated reflection/allocations for each subdraw without
        // accepting a range that any individual draw would access out of bounds.
        if let Some(maximum) = vertex_maximum {
            self.validate_instance_attributes(maximum, instance_maximum, batch.instanced)?;
        }
        let entries = self
            .extensions
            .multi_draw_entries
            .ok_or(gl::INVALID_OPERATION)?;
        let _sampling = self.sampling_guard()?;
        let draws = batch.counts.len() as i32;
        if batch.indexed {
            let offsets = batch
                .starts
                .iter()
                .map(|offset| *offset as usize as *const c_void)
                .collect::<Vec<_>>();
            // SAFETY: owned lists remain alive across the synchronous call;
            // each offset/range is checked against the bound element buffer.
            unsafe {
                if batch.instanced {
                    (entries.elements_instanced)(
                        batch.mode,
                        batch.counts.as_ptr(),
                        batch.kind,
                        offsets.as_ptr(),
                        batch.instances.as_ptr(),
                        draws,
                    );
                } else {
                    (entries.elements)(
                        batch.mode,
                        batch.counts.as_ptr(),
                        batch.kind,
                        offsets.as_ptr(),
                        draws,
                    );
                }
            }
        } else {
            // SAFETY: owned, bounded, prevalidated lists, with the exact pinned ABI.
            unsafe {
                if batch.instanced {
                    (entries.arrays_instanced)(
                        batch.mode,
                        batch.starts.as_ptr(),
                        batch.counts.as_ptr(),
                        batch.instances.as_ptr(),
                        draws,
                    );
                } else {
                    (entries.arrays)(
                        batch.mode,
                        batch.starts.as_ptr(),
                        batch.counts.as_ptr(),
                        draws,
                    );
                }
            }
        }
        self.driver_result()?;
        self.record_transform_capture(captured);
        Ok(Value::Null)
    }
}
