use super::*;

fn run(source: &str) -> (Option<WorkerRuntime>, WorkerRuntimeOutcome) {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    WorkerRuntime::start(
        "https://example.com/worker.js",
        source,
        "",
        ScriptKind::Classic,
        loader,
    )
}

#[test]
fn worker_file_reader_sync_reads_snapshot_bytes_and_decodes_declared_charset() {
    let (_, outcome) = run(r#"const source = new Uint8Array([65, 255, 0]);
            const blob = new Blob([source], {type:'TEXT/PLAIN;CHARSET=WINDOWS-1252'});
            source[0] = 99;
            const reader = new FileReaderSync();
            const buffer = reader.readAsArrayBuffer(blob);
            const text = reader.readAsText(blob);
            const utf8 = reader.readAsText(blob, 'utf-8');
            const binary = reader.readAsBinaryString(blob);
            const url = reader.readAsDataURL(blob);
            postMessage([
                [...new Uint8Array(buffer)].join(','), text.charCodeAt(0),
                text.charCodeAt(1), text.charCodeAt(2),
                utf8.charCodeAt(1), [...binary].map(c => c.charCodeAt(0)).join(','),
                url, Object.prototype.toString.call(reader)
            ].join('|'));"#);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result: String = serde_json::from_str(&outcome.messages[0]).unwrap();
    assert_eq!(
        result,
        "65,255,0|65|255|0|65533|65,255,0|data:text/plain;charset=windows-1252;base64,Qf8A|[object FileReaderSync]"
    );
}

#[test]
fn worker_file_reader_sync_rejects_foreign_receivers_and_non_blobs() {
    let (_, outcome) = run(
        r#"const reader = new FileReaderSync(), blob = new Blob(['ok']);
            const errorName = action => { try { action(); return 'none'; }
                catch (error) { return error.name; } };
            postMessage([
                errorName(() => reader.readAsArrayBuffer({})),
                errorName(() => FileReaderSync.prototype.readAsText.call({}, blob)),
                errorName(() => reader.readAsText(blob, Symbol())),
                reader.readAsText(blob),
                typeof document
            ].join('|'));"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result: String = serde_json::from_str(&outcome.messages[0]).unwrap();
    assert_eq!(result, "TypeError|TypeError|TypeError|ok|undefined");
}

#[test]
fn asynchronous_file_reader_remains_available_in_worker() {
    let (runtime, initial) = run(r#"const reader = new FileReader();
            reader.onload = () => postMessage(reader.result);
            reader.readAsText(new Blob(['worker read'], {type:'text/plain'}));"#);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.expect("worker failed to start");
    let completion = runtime.advance_time(Duration::ZERO, 8);
    assert!(completion.errors.is_empty(), "{:?}", completion.errors);
    let result: String = serde_json::from_str(&completion.messages[0]).unwrap();
    assert_eq!(result, "worker read");
}
