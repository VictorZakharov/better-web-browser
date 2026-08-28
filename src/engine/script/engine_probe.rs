//! Comparative JavaScript-engine probe used by the issue #113 architecture spike.

use crate::renderer_protocol::JavaScriptEngineProbe;
use boa_engine::{Context, Source};
use serde::Serialize;
use std::time::Instant;

const WORKLOAD: &str = include_str!("engine_probe_workload.js");

#[derive(Serialize)]
struct ProbeMeasurement {
    engine: &'static str,
    setup_micros: u64,
    evaluation_micros: u64,
    total_micros: u64,
    checksum: i64,
}

trait ProbeEngine {
    fn evaluate(self, source: &str) -> Result<ProbeMeasurement, String>;
}

struct BoaProbe;

impl ProbeEngine for BoaProbe {
    fn evaluate(self, source: &str) -> Result<ProbeMeasurement, String> {
        let total_started = Instant::now();
        let setup_started = Instant::now();
        let mut context = Context::default();
        let setup_micros = micros(setup_started.elapsed());
        let evaluation_started = Instant::now();
        let value = context
            .eval(Source::from_bytes(source))
            .map_err(|error| format!("Boa probe evaluation failed: {error}"))?;
        let checksum = i64::from(
            value
                .to_i32(&mut context)
                .map_err(|error| format!("Boa probe result conversion failed: {error}"))?,
        );

        Ok(ProbeMeasurement {
            engine: "boa",
            setup_micros,
            evaluation_micros: micros(evaluation_started.elapsed()),
            total_micros: micros(total_started.elapsed()),
            checksum,
        })
    }
}

struct V8Probe {
    jitless: bool,
}

impl ProbeEngine for V8Probe {
    fn evaluate(self, source: &str) -> Result<ProbeMeasurement, String> {
        let total_started = Instant::now();
        let setup_started = Instant::now();
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        if self.jitless {
            v8::V8::set_flags_from_string("--jitless");
        }
        v8::V8::initialize();

        let mut setup_micros = 0;
        let mut evaluation_micros = 0;
        let result: Result<i64, String> = (|| {
            let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
            v8::scope!(let handle_scope, isolate);
            let context = v8::Context::new(handle_scope, Default::default());
            let scope = &v8::ContextScope::new(handle_scope, context);
            setup_micros = micros(setup_started.elapsed());

            let evaluation_started = Instant::now();
            let code = v8::String::new(scope, source)
                .ok_or_else(|| "V8 probe could not allocate the script source".to_string())?;
            let script = v8::Script::compile(scope, code, None)
                .ok_or_else(|| "V8 probe compilation failed".to_string())?;
            let value = script
                .run(scope)
                .ok_or_else(|| "V8 probe evaluation failed".to_string())?;
            let checksum = value
                .integer_value(scope)
                .ok_or_else(|| "V8 probe did not return an integer checksum".to_string())?;
            evaluation_micros = micros(evaluation_started.elapsed());
            Ok(checksum)
        })();

        unsafe {
            v8::V8::dispose();
        }
        v8::V8::dispose_platform();

        let checksum = result?;
        Ok(ProbeMeasurement {
            engine: if self.jitless { "v8-jitless" } else { "v8-jit" },
            setup_micros,
            evaluation_micros,
            total_micros: micros(total_started.elapsed()),
            checksum,
        })
    }
}

pub(crate) fn run_engine_probe(engine: JavaScriptEngineProbe) -> Result<String, String> {
    let measurement = match engine {
        JavaScriptEngineProbe::Boa => BoaProbe.evaluate(WORKLOAD),
        JavaScriptEngineProbe::V8Jitless => V8Probe { jitless: true }.evaluate(WORKLOAD),
        JavaScriptEngineProbe::V8Jit => V8Probe { jitless: false }.evaluate(WORKLOAD),
    }?;
    serde_json::to_string(&measurement)
        .map_err(|error| format!("serialize JavaScript engine probe: {error}"))
}

fn micros(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}
