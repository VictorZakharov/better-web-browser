use super::*;

fn check(source: &str) {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, result) = WorkerRuntime::start(
        "https://example.test/worker.js",
        source,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(runtime.is_some());
    assert_eq!(result.messages, ["true"]);
}

#[test]
fn binary_clone_does_not_call_author_base64_functions() {
    check(
        r#"
        btoa = atob = () => { throw Error('author codec must not run'); };
        const bytes = new Uint8Array([0, 1, 127, 128, 254, 255]);
        const copy = structuredClone(bytes);
        if (copy.length !== bytes.length || !bytes.every((v,i) => copy[i] === v)) throw Error('pixels');
        postMessage(true);
    "#,
    );
}

#[test]
fn binary_clone_handles_large_texture_maps_under_the_normal_watchdog() {
    check(
        r#"
        const size = 4 * 1024 * 1024;
        const maps = [new Uint8ClampedArray(size), new Uint8ClampedArray(size), new Uint8ClampedArray(size)];
        maps.forEach((map, index) => { map[0] = index + 1; map[size-1] = 250-index; });
        const copy = structuredClone(maps, {transfer: maps.map(map => map.buffer)});
        copy.forEach((map, index) => {
            if (!(map instanceof Uint8ClampedArray) || map.length !== size ||
                map[0] !== index+1 || map[size-1] !== 250-index || maps[index].byteLength !== 0)
                throw Error('texture transport');
        });
        postMessage(true);
    "#,
    );
}

#[test]
fn native_clone_bytes_preserve_view_offsets_and_reject_invalid_inputs() {
    check(
        r#"
        const bytes = new Uint8Array([99,0,1,254,255,88]);
        const encoded = __hostCall('cloneBinaryEncode', bytes.subarray(1,5));
        if (encoded !== 'AAH+/w==') throw Error('view offset');
        const decoded = __hostCall('cloneBinaryDecode', encoded);
        if (!(decoded instanceof Uint8Array) || decoded.length !== 4 || decoded[3] !== 255) throw Error('decode');
        for (const input of ['?', 'AA=A', '!!!!', 'A']) {
            let caught = false;
            try { __hostCall('cloneBinaryDecode', input); } catch (error) { caught = error instanceof TypeError; }
            if (!caught) throw Error('bad encoding accepted');
        }
        let caught = false;
        try { __hostCall('cloneBinaryEncode', new Uint8Array(16*1024*1024+1)); }
        catch (error) { caught = error instanceof TypeError; }
        if (!caught) throw Error('unbounded allocation');
        postMessage(true);
    "#,
    );
}
