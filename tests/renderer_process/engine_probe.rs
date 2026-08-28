use super::support::{SERIAL, options};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    JavaScriptEngineProbe, RENDERER_DIAGNOSTIC_ENGINE_PROBE, TestCommand,
};
use serde_json::Value;
use std::net::TcpListener;
use std::time::Duration;

#[test]
fn contained_v8_probe_matches_boa_and_retains_non_jit_restrictions() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let boa = run_probe(JavaScriptEngineProbe::Boa, false);
    let v8_jitless = run_probe(JavaScriptEngineProbe::V8Jitless, false);
    let v8_jit = run_probe(JavaScriptEngineProbe::V8Jit, true);

    assert_eq!(boa["checksum"], v8_jitless["checksum"]);
    assert_eq!(boa["checksum"], v8_jit["checksum"]);

    let boa_total = boa["total_micros"].as_u64().unwrap();
    let v8_total = v8_jit["total_micros"].as_u64().unwrap();
    assert!(boa_total > 0);
    assert!(v8_total > 0);

    eprintln!(
        "engine probe total: boa={}us v8-jitless={}us v8-jit={}us boa/v8-jit={:.2}x",
        boa_total,
        v8_jitless["total_micros"].as_u64().unwrap(),
        v8_total,
        boa_total as f64 / v8_total as f64
    );
    eprintln!(
        "engine probe evaluation: boa={}us v8-jitless={}us v8-jit={}us boa/v8-jit={:.2}x",
        boa["evaluation_micros"].as_u64().unwrap(),
        v8_jitless["evaluation_micros"].as_u64().unwrap(),
        v8_jit["evaluation_micros"].as_u64().unwrap(),
        boa["evaluation_micros"].as_u64().unwrap() as f64
            / v8_jit["evaluation_micros"].as_u64().unwrap() as f64
    );
    eprintln!(
        "engine probe setup: boa={}us v8-jitless={}us v8-jit={}us",
        boa["setup_micros"].as_u64().unwrap(),
        v8_jitless["setup_micros"].as_u64().unwrap(),
        v8_jit["setup_micros"].as_u64().unwrap()
    );
}

fn run_probe(engine: JavaScriptEngineProbe, permit_dynamic_code: bool) -> Value {
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(120);
    launch.unresponsive_kill_timeout = Duration::from_secs(5);
    if permit_dynamic_code {
        launch.permit_dynamic_code_for_engine_spike();
    }

    let mut session = RendererSession::launch(launch).expect("launch contained engine probe");
    if permit_dynamic_code {
        assert_retained_restrictions(&session);
    }
    session
        .send_test_command(TestCommand::ProbeJavaScriptEngine { engine })
        .expect("send engine probe");

    let report = loop {
        match session
            .wait_for_event(Duration::from_secs(120))
            .expect("wait for engine probe")
        {
            RendererEvent::Diagnostic { code, text }
                if code == RENDERER_DIAGNOSTIC_ENGINE_PROBE =>
            {
                break serde_json::from_str(&text).expect("parse engine probe report");
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::Unresponsive => {}
            event => panic!("unexpected engine probe event: {event:?}"),
        }
    };
    session.shutdown().expect("shutdown engine probe");
    report
}

fn assert_retained_restrictions(session: &RendererSession) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback probe");
    let report = session
        .probe_restrictions(
            listener.local_addr().unwrap().port(),
            Duration::from_secs(3),
        )
        .expect("probe retained restrictions");

    assert!(report.child_launch_denied, "{report:?}");
    assert!(report.loopback_denied, "{report:?}");
    assert!(report.internet_denied, "{report:?}");
}
