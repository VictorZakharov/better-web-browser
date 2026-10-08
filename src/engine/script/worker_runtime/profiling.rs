//! Bounded, opt-in attribution for startup worker computation and GC.
use super::*;
use crate::engine::script::engine::owner_cpu as cpu;
use std::time::Instant;

pub(super) struct TaskSample {
    started: Instant,
    gc: (u64, Duration),
    cpu: Option<Duration>,
}

impl WorkerRuntime {
    /// No author source, arguments, result bytes, or function names are recorded.
    /// An execution limit remains the same whether diagnostics are on or off.
    pub fn set_execution_diagnostics(&mut self, enabled: bool) {
        if self.execution_profiling == enabled {
            return;
        }
        self.execution_profiling = enabled;
        self.remaining_diagnostic_samples = if enabled { 8 } else { 0 };
        self.remaining_failed_diagnostic_sample = enabled;
        self.context.set_execution_profiling(enabled);
    }

    pub(super) fn start_task_sample(&self) -> Option<TaskSample> {
        self.execution_profiling.then(|| TaskSample {
            started: Instant::now(),
            gc: self.context.gc_sample(),
            cpu: cpu::sample(),
        })
    }

    pub(super) fn finish_task_sample(
        &mut self,
        sample: Option<TaskSample>,
        outcome: &mut WorkerRuntimeOutcome,
    ) {
        let Some(sample) = sample else {
            return;
        };
        outcome
            .diagnostics
            .extend(self.context.take_cpu_task_diagnostics());
        // Reserve one failure sample independently of successful work. A
        // failing streaming worker must not grow its diagnostic log forever.
        if outcome.errors.is_empty() {
            if self.remaining_diagnostic_samples == 0 {
                return;
            }
            self.remaining_diagnostic_samples -= 1;
        } else {
            if !self.remaining_failed_diagnostic_sample {
                return;
            }
            self.remaining_failed_diagnostic_sample = false;
        }
        let elapsed = sample.started.elapsed();
        let gc = self.context.gc_sample();
        let cpu = sample.cpu.zip(cpu::sample()).map_or_else(
            || "unavailable".into(),
            |(before, after)| {
                format!(
                    "{:.3} ms",
                    after.saturating_sub(before).as_secs_f64() * 1000.0
                )
            },
        );
        outcome.diagnostics.push(format!(
            "message task: {:.3} ms elapsed; {cpu} owner-thread CPU; {} collections, {:.3} ms GC callback span",
            elapsed.as_secs_f64() * 1000.0,
            gc.0.saturating_sub(sample.gc.0),
            gc.1.saturating_sub(sample.gc.1).as_secs_f64() * 1000.0,
        ));
        match self.context.heap_diagnostic() {
            Ok(heap) => outcome.diagnostics.push(heap),
            Err(error) => outcome
                .diagnostics
                .push(format!("could not sample worker heap: {error}")),
        }
        outcome
            .diagnostics
            .extend(self.context.gpu_resource_diagnostics());
    }
}
