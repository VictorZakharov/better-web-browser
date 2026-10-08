//! CSS Variables 1 §3.3 requires bounded expansion, including small recursive
//! inputs whose substituted result grows exponentially.
//! https://www.w3.org/TR/css-variables-1/#long-variables
use super::*;
use std::fmt::{self, Write};

pub(super) const MAX_BYTES: usize = 1024 * 1024;
const MAX_SOURCE_WORK: usize = 4 * MAX_BYTES;
const MAX_SERIALIZATION_WORK: usize = 16 * MAX_BYTES;
const MAX_TOKENS: usize = 65_536;

#[derive(Default)]
pub(super) struct Budget {
    tokens: usize,
    source_work: usize,
    serialization_work: usize,
    pub(super) exhausted: bool,
}

impl Budget {
    pub(super) fn source(&mut self, value: &str, depth: usize) -> Option<()> {
        let spent = self.source_work.checked_add(value.len());
        if self.exhausted
            || value.len() > MAX_BYTES
            || depth > 32
            || spent.is_none_or(|bytes| bytes > MAX_SOURCE_WORK)
        {
            self.exhausted = true;
            return None;
        }
        // Large comments/whitespace can consume CPU without increasing the
        // serialized result or token count. Charge every referenced source.
        self.source_work = spent?;
        Some(())
    }

    pub(super) fn step(&mut self) -> Option<()> {
        if self.exhausted || self.tokens >= MAX_TOKENS {
            self.exhausted = true;
            return None;
        }
        self.tokens += 1;
        Some(())
    }

    pub(super) fn push(&mut self, output: &mut String, value: &str) -> Option<()> {
        let spent = self.serialization_work.checked_add(value.len());
        if self.exhausted
            || spent.is_none_or(|bytes| bytes > MAX_SERIALIZATION_WORK)
            || output
                .len()
                .checked_add(value.len())
                .is_none_or(|length| length > MAX_BYTES)
        {
            self.exhausted = true;
            return None;
        }
        // Include intermediate copies, not only the eventual winner's size.
        self.serialization_work = spent?;
        output.push_str(value);
        Some(())
    }

    pub(super) fn token(&mut self, output: &mut String, token: &Token<'_>) -> Option<()> {
        if token
            .to_css(&mut BoundedWriter {
                output,
                budget: self,
            })
            .is_err()
        {
            self.exhausted = true;
            return None;
        }
        Some(())
    }
}

struct BoundedWriter<'a> {
    output: &'a mut String,
    budget: &'a mut Budget,
}
impl Write for BoundedWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.budget.push(self.output, value).ok_or(fmt::Error)
    }
}
