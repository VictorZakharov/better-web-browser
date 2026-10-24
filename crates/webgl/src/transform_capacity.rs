//! Browser-owned capture accounting for atomic multi-draw admission.
//! The pinned provider validates each subdraw against the same initial cursor;
//! its native execution still owns the actual capture and buffer contents.
use super::{ApiVersion, Kind, Result, WebGl, gl};

#[derive(Default)]
pub(super) struct Ledger {
    pub mode: u32,
    pub capacity: u64,
    pub written: u64,
}

fn components(kind: u32) -> Option<u64> {
    Some(match kind {
        gl::FLOAT | gl::INT | gl::UNSIGNED_INT => 1,
        gl::FLOAT_VEC2 | gl::INT_VEC2 | 0x8dc6 => 2,
        gl::FLOAT_VEC3 | gl::INT_VEC3 | 0x8dc7 => 3,
        gl::FLOAT_VEC4 | gl::INT_VEC4 | 0x8dc8 | gl::FLOAT_MAT2 => 4,
        gl::FLOAT_MAT3 => 9,
        gl::FLOAT_MAT4 => 16,
        0x8b65 | 0x8b67 => 6,
        0x8b66 | 0x8b69 => 8,
        0x8b68 | 0x8b6a => 12,
        _ => return None,
    })
}

impl Ledger {
    fn prepare(
        &self,
        mode: u32,
        indexed: bool,
        draws: impl Iterator<Item = (u32, u32)>,
    ) -> Result<u64> {
        // WebGL2, based on GLES3, permits only non-indexed draws of the exact
        // primitive mode selected at beginTransformFeedback, without a GS.
        if indexed || mode != self.mode {
            return Err(gl::INVALID_OPERATION);
        }
        let primitive = match mode {
            gl::POINTS => 1,
            gl::LINES => 2,
            gl::TRIANGLES => 3,
            _ => return Err(gl::INVALID_OPERATION),
        };
        let mut vertices = 0u64;
        for (count, instances) in draws {
            // Incomplete primitives produce no capture, separately per draw
            // and instance. Do not round a sum across independent subdraws.
            let complete = u64::from(count - count % primitive) * u64::from(instances);
            vertices = vertices
                .checked_add(complete)
                .ok_or(gl::INVALID_OPERATION)?;
        }
        if self
            .written
            .checked_add(vertices)
            .is_none_or(|end| end > self.capacity)
        {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(vertices)
    }
}

impl WebGl {
    pub(super) fn new_transform_ledger(&mut self, mode: u32) -> Result<Ledger> {
        let native = self.objects.get(self.program, Kind::Program)?.native;
        let (mut count, mut storage_mode, mut name_size) = (0, 0, 0);
        unsafe {
            gl::GetProgramiv(native, 0x8c83, &mut count);
            gl::GetProgramiv(native, 0x8c7f, &mut storage_mode);
            gl::GetProgramiv(native, 0x8c76, &mut name_size);
        }
        self.driver_result()?;
        if !(1..=64).contains(&count) || !(1..=1025).contains(&name_size) {
            return Err(gl::INVALID_OPERATION);
        }
        let separate = storage_mode == 0x8c8d;
        if !separate && storage_mode != 0x8c8c {
            return Err(gl::INVALID_OPERATION);
        }
        let entries = &self.core.as_ref().ok_or(gl::INVALID_OPERATION)?.transform;
        let mut strides = Vec::with_capacity(count as usize);
        let mut name = vec![0i8; name_size as usize];
        for index in 0..count as u32 {
            let (mut length, mut size, mut kind) = (0, 0, 0);
            // SAFETY: bounded native reflection, trusted entry ABI and exactly
            // sized writable storage. No author pointers/names are passed here.
            unsafe {
                (entries.varying)(
                    native,
                    index,
                    name_size,
                    &mut length,
                    &mut size,
                    &mut kind,
                    name.as_mut_ptr(),
                );
            }
            if length < 0 || length >= name_size || size < 1 {
                return Err(gl::INVALID_OPERATION);
            }
            let bytes = components(kind)
                .and_then(|n| n.checked_mul(size as u64))
                .and_then(|n| n.checked_mul(4))
                .ok_or(gl::INVALID_OPERATION)?;
            strides.push(bytes);
        }
        self.driver_result()?;
        if !separate {
            strides = vec![
                strides
                    .iter()
                    .try_fold(0u64, |sum, n| sum.checked_add(*n))
                    .ok_or(gl::INVALID_OPERATION)?,
            ];
        }
        let record = &self.transform_feedback.records[&self.transform_feedback.bound];
        let mut capacity = u64::MAX;
        for (index, stride) in strides.into_iter().enumerate() {
            let binding = record.bindings.get(index).ok_or(gl::INVALID_OPERATION)?;
            let buffer = self.objects.get(binding.id, Kind::Buffer)?;
            let available = buffer.bytes.len().saturating_sub(binding.offset);
            let available = binding.size.map_or(available, |n| n.min(available));
            capacity = capacity.min(available as u64 / stride);
        }
        Ok(Ledger {
            mode,
            capacity,
            written: 0,
        })
    }

    pub(super) fn prepare_transform_capture(
        &self,
        mode: u32,
        indexed: bool,
        draws: impl Iterator<Item = (u32, u32)>,
    ) -> Result<u64> {
        if self.options.api == ApiVersion::One {
            return Ok(0);
        }
        let record = &self.transform_feedback.records[&self.transform_feedback.bound];
        if !record.active || record.paused {
            return Ok(0);
        }
        record.ledger.prepare(mode, indexed, draws)
    }

    pub(super) fn record_transform_capture(&mut self, vertices: u64) {
        if vertices == 0 {
            return;
        }
        let record = self
            .transform_feedback
            .records
            .get_mut(&self.transform_feedback.bound)
            .expect("bound feedback owner");
        // prepare_transform_capture checked addition and capacity before the
        // native draw. Only a successful native call advances this cursor.
        record.ledger.written += vertices;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_rounds_each_primitive_before_instances_and_batch_sum() {
        let ledger = Ledger {
            mode: gl::TRIANGLES,
            capacity: 12,
            written: 0,
        };
        assert_eq!(
            ledger.prepare(gl::TRIANGLES, false, [(4, 2), (5, 2)].into_iter()),
            Ok(12)
        );
        assert_eq!(
            ledger.prepare(gl::TRIANGLES, false, [(2, 1000)].into_iter()),
            Ok(0)
        );
        assert_eq!(
            ledger.prepare(gl::TRIANGLES, false, [(3, 5)].into_iter()),
            Err(gl::INVALID_OPERATION)
        );
    }

    #[test]
    fn capture_counts_previous_writes_and_checks_mode_and_indexed_draws() {
        let ledger = Ledger {
            mode: gl::POINTS,
            capacity: 5,
            written: 3,
        };
        assert_eq!(
            ledger.prepare(gl::POINTS, false, [(1, 1), (1, 1)].into_iter()),
            Ok(2)
        );
        assert_eq!(
            ledger.prepare(gl::POINTS, false, [(1, 1), (2, 1)].into_iter()),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            ledger.prepare(gl::POINTS, true, [(0, 0)].into_iter()),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            ledger.prepare(gl::LINES, false, [(0, 0)].into_iter()),
            Err(gl::INVALID_OPERATION)
        );
    }

    #[test]
    fn varying_components_cover_scalar_vectors_and_non_square_matrices() {
        for (kind, count) in [
            (gl::FLOAT, 1),
            (gl::INT_VEC3, 3),
            (0x8dc8, 4),
            (gl::FLOAT_MAT4, 16),
            (0x8b65, 6),
            (0x8b66, 8),
            (0x8b67, 6),
            (0x8b68, 12),
            (0x8b69, 8),
            (0x8b6a, 12),
        ] {
            assert_eq!(components(kind), Some(count));
        }
        assert_eq!(components(gl::SAMPLER_2D), None);
    }
}
