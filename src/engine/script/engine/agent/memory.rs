//! Owner-only native policy after a complete task and its Promise checkpoint.
use super::super::memory_pressure::{Advice, Level};
use super::*;
use std::time::Instant;

impl Agent {
    pub(in crate::engine::script::engine) fn memory_pressure_checkpoint(&mut self) -> JsResult<()> {
        let Some(advice) = super::super::memory_pressure::advice() else {
            return Ok(());
        };
        self.deliver_memory_pressure(advice)
    }

    fn deliver_memory_pressure(&mut self, advice: Advice) -> JsResult<()> {
        if !self.pressure.pending(advice) {
            return Ok(());
        }
        let started = Instant::now();
        // Use the normal cancellation/entry guard, not an author-visible hook.
        // Do not clear kept WeakRef objects, pump tasks or run Promise reactions here.
        // https://github.com/v8/v8/blob/main/include/v8-isolate.h
        self.watchdog.run(&mut self.isolate, |isolate| {
            let level = match advice.level {
                Level::None => v8::MemoryPressureLevel::None,
                Level::Moderate => v8::MemoryPressureLevel::Moderate,
                Level::Critical => v8::MemoryPressureLevel::Critical,
            };
            // A cooled growth pulse must reach V8 even if its previous level
            // was unchanged. The process policy already bounds these pulses.
            if advice.level != Level::None {
                isolate.memory_pressure_notification(v8::MemoryPressureLevel::None);
            }
            isolate.memory_pressure_notification(level);
            Ok(())
        })?;
        self.pressure.delivered(advice);
        self.pressure_profile
            .record(advice.level, started.elapsed(), advice.private);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
