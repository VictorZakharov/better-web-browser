//! Opt-in native-owner timing, separate from realm bridge and mailbox wait time.
//! Fixed categories never retain author commands, source, names, handles or bytes.
use crate::engine::owner_cpu;
use std::time::{Duration, Instant};

const CATEGORIES: usize = 8;

#[derive(Clone, Copy)]
pub(super) enum Category {
    ShaderSource,
    Compile,
    Link,
    Query,
    Upload,
    Draw,
    Readback,
    State,
}

impl Category {
    const ALL: [Self; CATEGORIES] = [
        Self::ShaderSource,
        Self::Compile,
        Self::Link,
        Self::Query,
        Self::Upload,
        Self::Draw,
        Self::Readback,
        Self::State,
    ];

    pub(super) fn command(operation: &str) -> Self {
        match operation {
            "shaderSource" => Self::ShaderSource,
            "compileShader" => Self::Compile,
            "linkProgram" => Self::Link,
            "readPixels" | "getBufferSubData" => Self::Readback,
            "bufferData"
            | "bufferSubData"
            | "texImage2D"
            | "texSubImage2D"
            | "texImage3D"
            | "texSubImage3D"
            | "texStorage2D"
            | "texStorage3D"
            | "compressedTexImage2D"
            | "compressedTexSubImage2D"
            | "compressedTexImage3D"
            | "compressedTexSubImage3D"
            | "copyTexImage2D"
            | "copyTexSubImage2D"
            | "copyTexSubImage3D"
            | "copyBufferSubData"
            | "renderbufferStorage"
            | "renderbufferStorageMultisample"
            | "generateMipmap" => Self::Upload,
            "drawArrays"
            | "drawElements"
            | "drawArraysInstanced"
            | "drawElementsInstanced"
            | "drawRangeElements"
            | "multiDrawArraysWEBGL"
            | "multiDrawElementsWEBGL"
            | "multiDrawArraysInstancedWEBGL"
            | "multiDrawElementsInstancedWEBGL"
            | "clear"
            | "clearBufferfv"
            | "clearBufferiv"
            | "clearBufferuiv"
            | "clearBufferfi"
            | "blitFramebuffer" => Self::Draw,
            value if value.starts_with("get") || value.starts_with("is") => Self::Query,
            _ => Self::State,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ShaderSource => "shader-source",
            Self::Compile => "validation/compile",
            Self::Link => "link",
            Self::Query => "query",
            Self::Upload => "storage/upload",
            Self::Draw => "draw/clear",
            Self::Readback => "readback/presentation",
            Self::State => "state",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Stats {
    calls: u64,
    elapsed: Duration,
    maximum: Duration,
    cpu: Duration,
    cpu_samples: u64,
}

pub(super) struct Sample {
    started: Instant,
    cpu: Option<Duration>,
}

#[derive(Default)]
pub(super) struct Profile {
    enabled: bool,
    stats: [Stats; CATEGORIES],
    peak_charged: usize,
}

impl Profile {
    pub(super) fn enable(&mut self, enabled: bool, charged: usize) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.stats = [Stats::default(); CATEGORIES];
        self.peak_charged = if enabled { charged } else { 0 };
    }

    pub(super) fn start(&self) -> Option<Sample> {
        // No clock/OS calls in ordinary browsing; diagnostics are not a speed benchmark.
        self.enabled.then(|| Sample {
            started: Instant::now(),
            cpu: owner_cpu::sample(),
        })
    }

    pub(super) fn finish(&mut self, category: Category, sample: Option<Sample>, charged: usize) {
        let Some(sample) = sample.filter(|_| self.enabled) else {
            return;
        };
        self.record(
            category,
            sample.started.elapsed(),
            sample
                .cpu
                .zip(owner_cpu::sample())
                .map(|(before, after)| after.saturating_sub(before)),
            charged,
        );
    }

    fn record(
        &mut self,
        category: Category,
        elapsed: Duration,
        cpu: Option<Duration>,
        charged: usize,
    ) {
        let stats = &mut self.stats[category as usize];
        stats.calls = stats.calls.saturating_add(1);
        stats.elapsed = stats.elapsed.saturating_add(elapsed);
        stats.maximum = stats.maximum.max(elapsed);
        if let Some(cpu) = cpu {
            stats.cpu = stats.cpu.saturating_add(cpu);
            stats.cpu_samples = stats.cpu_samples.saturating_add(1);
        }
        self.peak_charged = self.peak_charged.max(charged);
    }

    pub(super) fn take(&mut self, id: u32) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let stats = std::mem::replace(&mut self.stats, [Stats::default(); CATEGORIES]);
        let mut output = vec![format!(
            "WebGL {id} native-owner charged high-water={}",
            self.peak_charged
        )];
        for category in Category::ALL {
            let stats = stats[category as usize];
            if stats.calls == 0 {
                continue;
            }
            // CPU resolution may round a small call to zero; unknown is not zero.
            let cpu = if stats.cpu_samples == stats.calls {
                format!("{:.3} ms", stats.cpu.as_secs_f64() * 1000.0)
            } else {
                "unavailable".into()
            };
            output.push(format!(
                "WebGL {id} native-owner {}: {} calls, {:.3} ms elapsed, {:.3} ms max, {cpu} owner-thread CPU",
                category.label(), stats.calls, stats.elapsed.as_secs_f64() * 1000.0,
                stats.maximum.as_secs_f64() * 1000.0,
            ));
        }
        output
    }
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
