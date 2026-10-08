//! Optional aggregate timing for the native JavaScript bridge.

use super::*;

const MAX_DIAGNOSTICS: usize = 16;
const MAX_ATTRIBUTE_NAMES: usize = 64;
const MAX_OPERATION_NAMES: usize = 256;
const MIN_REPORTED_TIME: Duration = Duration::from_micros(100);

#[derive(Default)]
pub(super) struct HostCallProfile {
    enabled: bool,
    operations: HashMap<String, HostCallStats>,
    attribute_writes: HashMap<String, usize>,
}

#[derive(Default)]
struct HostCallStats {
    calls: usize,
    total: Duration,
    maximum: Duration,
}

impl HostCallProfile {
    pub(super) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.operations.clear();
            self.attribute_writes.clear();
        }
    }

    pub(super) fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub(super) fn record(&mut self, operation: &str, started: Option<Instant>) {
        if !self.enabled {
            return;
        }
        let Some(elapsed) = started.map(|started| started.elapsed()) else {
            return;
        };
        self.record_elapsed(operation, elapsed);
    }

    pub(super) fn record_elapsed(&mut self, operation: &str, elapsed: Duration) {
        if !self.enabled || elapsed.is_zero() {
            return;
        }
        // __hostCall is reachable by author code. Diagnostics must not retain
        // an unbounded set of arbitrary operation strings between checkpoints.
        if operation.len() > 128
            || (!self.operations.contains_key(operation)
                && self.operations.len() >= MAX_OPERATION_NAMES)
        {
            return;
        }
        let stats = self.operations.entry(operation.to_owned()).or_default();
        stats.calls += 1;
        stats.total += elapsed;
        stats.maximum = stats.maximum.max(elapsed);
    }

    pub(super) fn record_webgl_command(&mut self, args: &[JsValue], started: Option<Instant>) {
        if !self.enabled {
            return;
        }
        let Some(JsValue::String(command)) = args.get(2) else {
            return;
        };
        let Some(tail) = command.strip_prefix("{\"op\":\"") else {
            return;
        };
        let Some((name, suffix)) = tail.split_once('"') else {
            return;
        };
        if !suffix.starts_with([',', '}']) {
            return;
        }
        // Fixed labels only, never command bodies, shader text, or user data.
        let category = match name {
            "compileShader" => "webgl::compileShader",
            "linkProgram" => "webgl::linkProgram",
            "getShaderParameter" => "webgl::getShaderParameter",
            "getProgramParameter" => "webgl::getProgramParameter",
            "texImage2D" => "webgl::texImage2D",
            "texSubImage2D" => "webgl::texSubImage2D",
            "texImage3D" => "webgl::texImage3D",
            "generateMipmap" => "webgl::generateMipmap",
            "bufferData" => "webgl::bufferData",
            "shaderSource" => "webgl::shaderSource",
            "getParameter" => "webgl::getParameter",
            _ => "webgl::other",
        };
        // Includes draining earlier queued void commands. This is the author's
        // synchronous bridge latency, not exclusively a single driver call.
        self.record(category, started);
    }

    pub(super) fn record_attribute_write(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        let name = name.to_ascii_lowercase();
        if let Some(count) = self.attribute_writes.get_mut(&name) {
            *count = count.saturating_add(1);
        } else if self.attribute_writes.len() < MAX_ATTRIBUTE_NAMES {
            self.attribute_writes.insert(name, 1);
        }
    }

    pub(super) fn take_diagnostics(&mut self) -> Vec<String> {
        let mut operations = std::mem::take(&mut self.operations)
            .into_iter()
            .filter(|(_, stats)| stats.total >= MIN_REPORTED_TIME)
            .collect::<Vec<_>>();
        operations.sort_unstable_by(|left, right| {
            right
                .1
                .total
                .cmp(&left.1.total)
                .then_with(|| left.0.cmp(&right.0))
        });
        let mut diagnostics = operations
            .into_iter()
            .map(|(operation, stats)| {
                format!(
                    "host call {operation}: {} calls, {:.3} ms total, {:.3} ms max",
                    stats.calls,
                    stats.total.as_secs_f64() * 1_000.0,
                    stats.maximum.as_secs_f64() * 1_000.0,
                )
            })
            .collect::<Vec<_>>();
        let mut attributes = std::mem::take(&mut self.attribute_writes)
            .into_iter()
            .collect::<Vec<_>>();
        attributes.sort_unstable_by(|left, right| {
            right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
        });
        if !attributes.is_empty() {
            diagnostics.insert(
                0,
                format!(
                    "attribute writes: {}",
                    attributes
                        .into_iter()
                        .take(12)
                        .map(|(name, count)| format!("{name}:{count}"))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            );
        }
        diagnostics.truncate(MAX_DIAGNOSTICS);
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_profile_does_not_collect_and_enabled_profile_is_drained() {
        let mut profile = HostCallProfile::default();
        profile.record("query", Some(Instant::now() - Duration::from_millis(2)));
        assert!(profile.take_diagnostics().is_empty());

        profile.set_enabled(true);
        profile.record("query", Some(Instant::now() - Duration::from_millis(2)));
        profile.record_attribute_write("CLASS");
        let diagnostics = profile.take_diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0], "attribute writes: class:1");
        assert!(diagnostics[1].starts_with("host call query: 1 calls,"));
        assert!(profile.take_diagnostics().is_empty());
    }

    #[test]
    fn explicit_phase_durations_are_opt_in_and_aggregate_without_zero_work() {
        let mut profile = HostCallProfile::default();
        profile.record_elapsed("layoutFlush::style", Duration::from_millis(3));
        assert!(profile.take_diagnostics().is_empty());
        profile.set_enabled(true);
        profile.record_elapsed("layoutFlush::style", Duration::from_millis(3));
        profile.record_elapsed("layoutFlush::style", Duration::from_millis(2));
        profile.record_elapsed("layoutFlush::layout", Duration::ZERO);
        assert_eq!(
            profile.take_diagnostics(),
            vec!["host call layoutFlush::style: 2 calls, 5.000 ms total, 3.000 ms max"]
        );
        assert!(profile.take_diagnostics().is_empty());
    }

    #[test]
    fn operation_labels_cannot_grow_without_bound() {
        let mut profile = HostCallProfile::default();
        profile.set_enabled(true);
        for index in 0..MAX_OPERATION_NAMES + 50 {
            profile.record_elapsed(&format!("author-{index}"), Duration::from_millis(1));
        }
        profile.record_elapsed(&"x".repeat(129), Duration::from_millis(1));
        assert_eq!(profile.operations.len(), MAX_OPERATION_NAMES);
        profile.record_elapsed("author-0", Duration::from_millis(2));
        assert_eq!(profile.operations["author-0"].calls, 2);
        profile.take_diagnostics();
        assert!(profile.operations.is_empty());
    }

    #[test]
    fn webgl_breakdown_keeps_only_fixed_names_and_no_command_contents() {
        let mut profile = HostCallProfile::default();
        let command = |op: &str| {
            vec![
                JsValue::String("webglCommand".into()),
                JsValue::from(1),
                JsValue::String(format!(r#"{{"op":"{op}","text":"private shader source"}}"#)),
            ]
        };
        let started = Some(Instant::now() - Duration::from_millis(2));
        profile.record_webgl_command(&command("compileShader"), started);
        assert!(profile.operations.is_empty());
        profile.set_enabled(true);
        profile.record_webgl_command(&command("compileShader"), started);
        profile.record_webgl_command(&command("unknown-author-operation"), started);
        assert_eq!(profile.operations.len(), 2);
        let text = profile.take_diagnostics().join("\n");
        assert!(text.contains("webgl::compileShader"));
        assert!(text.contains("webgl::other"));
        assert!(!text.contains("private"));
        assert!(!text.contains("unknown-author"));
    }
}
