//! Worker numeric code runs in its own ordinary global realm. Tests check the
//! result and isolation contract; throughput belongs to the local HTML fixture.
use super::*;

fn start(source: &str) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/numeric-worker.js",
        source,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    (
        runtime.expect("numeric worker failed to initialize"),
        initial,
    )
}

#[test]
fn numeric_worker_globals_keep_native_math_and_typed_array_semantics() {
    let (_, outcome) = start(
        r#"
        const output = new Uint8ClampedArray(64 * 64 * 4);
        let checksum = 0;
        for (let y = 0; y < 64; y++) for (let x = 0; x < 64; x++) {
            const offset = (y * 64 + x) * 4;
            output[offset] = Math.hypot(x, y);
            output[offset + 1] = Math.sqrt(x * x + y * y);
            output[offset + 2] = Math.imul(x, y) & 255;
            output[offset + 3] = 255;
            if (output[offset] !== output[offset + 1]) throw Error('numeric pixel mismatch');
            checksum += output[offset + 3];
        }
        if (checksum !== 64 * 64 * 255) throw Error('incomplete output');
        const copy = structuredClone(output, {transfer:[output.buffer]});
        if (!(copy instanceof Uint8ClampedArray) || copy.length !== 64 * 64 * 4 || output.byteLength !== 0)
            throw Error('numeric transport');
        postMessage(true);
    "#,
    );
    assert_eq!(outcome.messages, ["true"]);
}

#[test]
fn worker_global_writes_do_not_change_another_workers_intrinsics() {
    let (_, changed) = start(
        r#"
        Object.defineProperty(globalThis, 'Math', {value:{hypot:()=>99}, configurable:true});
        if (Math.hypot(3,4) !== 99) throw Error('worker global write');
        globalThis.workerOnly = 17;
        postMessage(true);
    "#,
    );
    assert_eq!(changed.messages, ["true"]);
    let (_, independent) = start(
        r#"
        if (Math.hypot(3,4) !== 5 || 'workerOnly' in globalThis) throw Error('worker realm leak');
        postMessage(true);
    "#,
    );
    assert_eq!(independent.messages, ["true"]);
}

#[test]
fn numeric_globals_remain_available_in_successive_message_tasks() {
    let (mut worker, _) = start(
        r#"
        let calls = 0;
        onmessage = event => {
            const source = new Float32Array([event.data, ++calls]);
            const result = Math.hypot(source[0], source[1]);
            if (!Number.isFinite(result)) throw Error('numeric result');
            postMessage(calls);
        };
    "#,
    );
    for call in 1..=3 {
        let outcome = worker.dispatch_message("3");
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.messages, [call.to_string()]);
    }
}
