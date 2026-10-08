//! Explicit diagnostic-only statistical source coordinates at a failing task.
//! Default builds/runs do not create a profiler. Values are numeric and bounded;
//! no author names, strings, source, arguments or objects are inspected.
use std::ffi::c_void;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn breeze_v8_start_cpu_samples() -> *mut c_void;
    fn breeze_v8_stop_cpu_samples(recording: *mut c_void, result: *mut Summary);
}

#[derive(Default)]
pub(super) struct Profile {
    enabled: bool,
    failed: bool,
    completed: u8,
    diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Row {
    script: i32,
    line: i32,
    column: i32,
    hits: u32,
}

#[derive(Default)]
#[repr(C)]
struct Summary {
    total_hits: u32,
    non_script_hits: u32,
    omitted_hits: u32,
    truncated: u32,
    visited: u32,
    rows: u32,
    interval_us: u32,
    reserved: u32,
    source_hits: [u32; 6],
    entries: [Row; 16],
}

pub(super) struct Recording(*mut c_void, Instant);

impl Recording {
    // Called only inside the watchdog's entered-isolate closure. RAII stops
    // the native sampler before that entry guard exits, including on unwind.
    pub(super) fn start(enabled: bool) -> Option<Self> {
        if !enabled {
            return None;
        }
        // SAFETY: current isolate is initialized, entered and owner-thread-only.
        let pointer = unsafe { breeze_v8_start_cpu_samples() };
        (!pointer.is_null()).then_some(Self(pointer, Instant::now()))
    }

    fn finish(mut self) -> Summary {
        let mut result = Summary::default();
        // SAFETY: unique opaque recording from start, output is the identical
        // fixed repr(C) layout; native disposal completes while isolate entered.
        unsafe { breeze_v8_stop_cpu_samples(self.0, &mut result) };
        self.0 = std::ptr::null_mut();
        result
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: same unique lifetime as finish; no pointers escape.
            unsafe { breeze_v8_stop_cpu_samples(self.0, std::ptr::null_mut()) };
        }
    }
}

impl Profile {
    #[cfg(test)]
    pub(super) fn enable_for_test(&mut self) {
        self.enabled = true;
        self.completed = 2;
    }

    pub(super) fn enable(&mut self, enabled: bool) {
        let enabled = enabled
            && std::env::var_os("BREEZE_DIAGNOSTIC_CPU_SAMPLING").as_deref()
                == Some(std::ffi::OsStr::new("1"));
        if self.enabled != enabled {
            *self = Self {
                enabled,
                completed: 2,
                ..Self::default()
            };
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled && !self.failed
    }

    pub(super) fn finish(&mut self, recording: Option<Recording>, failed: bool) {
        let Some(recording) = recording else { return };
        if failed {
            self.record(recording.finish());
        } else if self.enabled()
            && self.completed > 0
            && recording.1.elapsed() >= Duration::from_millis(500)
        {
            self.completed -= 1;
            self.emit(recording.finish(), "completed slow task");
        } else {
            // Stop/delete profiles that will not be exported without walking
            // their trees. Draining diagnostics never replenishes the budget.
            drop(recording);
        }
    }

    fn record(&mut self, summary: Summary) {
        if !self.enabled || self.failed {
            return;
        }
        self.failed = true;
        self.emit(summary, "first failed task");
    }

    fn emit(&mut self, summary: Summary, category: &'static str) {
        self.diagnostics.push(format!(
            "engine CPU samples ({category}): {} hits; {} non-script, {} omitted; {} nodes; truncated={}; target interval {} us; source types script {}, builtin {}, callback {}, internal {}, unresolved {}, other {}; diagnostic overhead applies",
            summary.total_hits,summary.non_script_hits,summary.omitted_hits,
            summary.visited,summary.truncated != 0,summary.interval_us,
            summary.source_hits[0],summary.source_hits[1],summary.source_hits[2],
            summary.source_hits[3],summary.source_hits[4],summary.source_hits[5]));
        for row in summary.entries.iter().take((summary.rows as usize).min(16)) {
            self.diagnostics.push(format!(
                "engine CPU source: script {}, line {}, column {}, {} hits",
                row.script, row.line, row.column, row.hits
            ));
        }
    }

    pub(super) fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_slow_samples_are_bounded_without_consuming_the_failure_slot() {
        super::super::runtime::initialize_v8();
        let _isolate = v8::Isolate::new(v8::CreateParams::default());
        let mut profile = Profile::default();
        profile.enable_for_test();
        // Simulate elapsed time on the owned recording rather than sleeping
        // half a second per case or changing the process-global environment.
        for index in 0..4 {
            let mut recording = Recording::start(true).unwrap();
            recording.1 = Instant::now() - Duration::from_millis(501);
            profile.finish(Some(recording), false);
            let rows = profile.take();
            if index < 2 {
                assert_eq!(rows.len(), 1);
                assert!(rows[0].contains("completed slow task"));
            } else {
                assert!(rows.is_empty());
            }
        }
        assert!(profile.enabled());
        profile.record(Summary::default());
        assert!(profile.take()[0].contains("first failed task"));
        assert!(!profile.enabled());
    }

    #[test]
    fn real_javascript_samples_export_numeric_coordinates_not_author_strings() {
        super::super::runtime::initialize_v8();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        let recording = Recording::start(true).expect("native CPU profiler");
        {
            v8::scope!(let scope,&mut isolate);
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let source=v8::String::new(scope,
                "function privateSensitiveName(){const start=Date.now();let n=0;while(Date.now()-start<120)n++;return n}privateSensitiveName();").unwrap();
            let script = v8::Script::compile(scope, source, None).unwrap();
            assert!(script.run(scope).is_some());
        }
        let summary = recording.finish();
        assert!(
            summary.total_hits > 0 && summary.rows > 0,
            "real script must be sampled"
        );
        assert_eq!(summary.source_hits.iter().sum::<u32>(), summary.total_hits);
        assert!(
            summary.entries[..summary.rows as usize]
                .iter()
                .all(|row| row.script > 0 && row.line > 0 && row.hits > 0)
        );
        let mut profile = Profile {
            enabled: true,
            ..Profile::default()
        };
        profile.record(summary);
        let rows = profile.take();
        assert!(rows.len() <= 17);
        assert!(rows.iter().all(|row| !row.contains("privateSensitiveName")
            && !row.contains("Date.now")
            && !row.contains("function")));
    }

    #[test]
    fn native_sampler_disposes_before_isolate_teardown_and_after_unwind() {
        super::super::runtime::initialize_v8();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        for _ in 0..3 {
            let recording = Recording::start(true).expect("native CPU profiler");
            let summary = recording.finish();
            assert_eq!(summary.interval_us, 2000);
            assert!(summary.rows <= 16 && summary.visited <= 16384);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _recording = Recording::start(true).expect("native CPU profiler");
            panic!("exercise recording disposal while owning isolate remains entered");
        }));
        assert!(result.is_err());
        let recording = Recording::start(true).expect("sampler reusable after unwind");
        recording.finish();
        isolate.low_memory_notification();
    }

    #[test]
    fn numeric_profile_layout_and_failure_output_are_fixed_and_bounded() {
        assert_eq!(std::mem::size_of::<Row>(), 16);
        assert_eq!(std::mem::size_of::<Summary>(), 312);
        let mut profile = Profile::default();
        profile.record(Summary::default());
        assert!(profile.take().is_empty());
        profile.enabled = true;
        profile.record(Summary {
            rows: u32::MAX,
            total_hits: 100,
            interval_us: 2000,
            ..Summary::default()
        });
        assert_eq!(profile.take().len(), 17);
        for _ in 0..100 {
            profile.record(Summary::default());
        }
        assert!(profile.take().is_empty());
        assert!(!profile.enabled());
        assert!(Recording::start(false).is_none());
    }
}
