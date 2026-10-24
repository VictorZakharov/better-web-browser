//! Owned numeric operands; common state/vector/matrix records need no heap buffers.
use super::Pending;
use crate::Command;
use smallvec::SmallVec;
use std::borrow::Cow;

pub const MAX_NUMERIC_VALUES: usize = 64;
pub const MAX_UNIFORM_FLOATS: usize = 8192;

pub fn large_float_uniform(op: &str) -> bool {
    matches!(
        op,
        "uniform1fv"
            | "uniform2fv"
            | "uniform3fv"
            | "uniform4fv"
            | "uniformMatrix2fv"
            | "uniformMatrix3fv"
            | "uniformMatrix4fv"
            | "uniformMatrix2x3fv"
            | "uniformMatrix2x4fv"
            | "uniformMatrix3x2fv"
            | "uniformMatrix3x4fv"
            | "uniformMatrix4x2fv"
            | "uniformMatrix4x3fv"
    )
}

/// Only converted, reply-free setters. Native GL shape/state validation remains mandatory.
pub struct NumericCommand {
    op: Cow<'static, str>,
    i: SmallVec<[i64; 8]>,
    f: SmallVec<[f64; 16]>,
}

impl NumericCommand {
    pub fn new(op: String, i: Vec<i64>, f: Vec<f64>) -> Option<Self> {
        Self::admit(Cow::Owned(op), i.into(), f.into())
    }

    pub(crate) fn packet(
        op: &'static str,
        i: SmallVec<[i64; 8]>,
        f: SmallVec<[f64; 16]>,
    ) -> Option<Self> {
        Self::admit(Cow::Borrowed(op), i, f)
    }

    fn admit(op: Cow<'static, str>, i: SmallVec<[i64; 8]>, f: SmallVec<[f64; 16]>) -> Option<Self> {
        if !Pending::operation(&op)
            || (i.len().checked_add(f.len())? > MAX_NUMERIC_VALUES
                && !(large_float_uniform(&op) && i.len() <= 2 && f.len() <= MAX_UNIFORM_FLOATS))
            || i.iter()
                .any(|value| value.unsigned_abs() > 9_007_199_254_740_991)
        {
            return None;
        }
        Some(Self { op, i, f })
    }

    pub(crate) fn write_command(self, command: &mut Command) {
        // Reuse a single ordinary dispatcher record within this synchronous owner batch.
        // No dispatcher retains these borrowed operands after its GL call returns.
        match self.op {
            Cow::Owned(op) => command.op = op,
            Cow::Borrowed(op) => {
                command.op.clear();
                command.op.push_str(op);
            }
        }
        if self.i.spilled() {
            command.i = self.i.into_vec()
        } else {
            command.i.clear();
            command.i.extend_from_slice(&self.i);
        }
        if self.f.spilled() {
            command.f = self.f.into_vec()
        } else {
            command.f.clear();
            command.f.extend_from_slice(&self.f);
        }
        command.text.clear();
    }

    #[cfg(test)]
    pub(crate) fn into_command(self) -> Command {
        Command {
            op: self.op.into_owned(),
            i: self.i.into_vec(),
            f: self.f.into_vec(),
            text: String::new(),
        }
    }
}

#[cfg(test)]
mod tests;
