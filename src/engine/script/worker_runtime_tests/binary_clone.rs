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

// Test the codec boundary before bootstrap, not through an author-visible
// bridge. Public Worker tests above and below use the normal private bindings.
fn native_check(source: &str) {
    let host = Rc::new(RefCell::new(WorkerHostState::new(
        "https://example.test/worker.js",
        true,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
    )));
    let mut context = Context::new(HostBridge::Worker(Rc::downgrade(&host))).unwrap();
    let result = context.eval(Source::from_bytes(source)).unwrap();
    assert_eq!(result.string_value(), "true");
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
    native_check(
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
        true;
    "#,
    );
}

#[test]
fn cloned_views_share_one_backing_buffer_independent_of_graph_order() {
    check(
        r#"
        for (const bufferFirst of [false,true]) {
            const buffer=new ArrayBuffer(32),bytes=new Uint8Array(buffer,4,12);
            const words=new Uint16Array(buffer,6,4),data=new DataView(buffer,5,9);
            bytes[2]=17;
            const source=bufferFirst?{buffer,bytes,words,data}:{bytes,words,data,buffer};
            source.self=source;source.again=bytes;
            const copy=structuredClone(source);
            if(copy.self!==copy||copy.again!==copy.bytes||copy.buffer===buffer||
                copy.bytes.buffer!==copy.buffer||copy.words.buffer!==copy.buffer||
                copy.data.buffer!==copy.buffer||copy.bytes.byteOffset!==4||
                copy.words.byteOffset!==6||copy.data.byteOffset!==5||copy.data.byteLength!==9)
                throw Error('backing buffer graph identity');
            copy.data.setUint8(1,91);
            if(copy.bytes[2]!==91||bytes[2]!==17)throw Error('shared clone storage or isolated original');
        }
        postMessage(true);
    "#,
    );
}

#[test]
fn view_clone_uses_native_brand_and_slots_not_author_getters() {
    check(
        r#"
        class CustomBytes extends Uint8Array {}
        const source=new CustomBytes(new ArrayBuffer(12),3,5);
        source[0]=19;source[4]=73;
        for(const name of ['constructor','buffer','byteOffset','byteLength','length'])
            Object.defineProperty(source,name,{get(){throw Error(`author ${name} getter`);}});
        const NativeBytes=Uint8Array;
        Uint8Array=function(){throw Error('author replacement constructor');};
        ArrayBuffer.isView=()=>false;
        const copy=structuredClone(source);
        if(Object.getPrototypeOf(copy)!==NativeBytes.prototype||copy.byteOffset!==3||
            copy.length!==5||copy[0]!==19||copy[4]!==73)throw Error('native view slots');
        postMessage(true);
    "#,
    );
}

#[test]
fn transferred_backing_store_retains_all_view_aliases_and_detaches_originals() {
    check(
        r#"
        const buffer=new ArrayBuffer(24),a=new Uint8Array(buffer,4,8),b=new DataView(buffer,6,4);
        a[2]=99;
        const copy=structuredClone({a,b,buffer},{transfer:[buffer]});
        if(buffer.byteLength!==0||a.byteLength!==0||copy.a.buffer!==copy.buffer||
            copy.b.buffer!==copy.buffer||copy.b.getUint8(0)!==99)throw Error('transfer aliases');
        let rejected=false;
        try{structuredClone(a);}catch(e){rejected=e.name==='DataCloneError';}
        if(!rejected)throw Error('detached view cloned');
        postMessage(true);
    "#,
    );
}

#[test]
fn old_persisted_binary_view_envelopes_remain_readable() {
    check(
        r#"
        const copy=__deserializeClone(JSON.stringify({t:'view',id:1,c:'Uint8Array',b:'AQIDBA==',o:1,l:2}));
        if(copy.byteOffset!==1||copy.length!==2||copy[0]!==2||copy[1]!==3)
            throw Error('legacy view envelope');
        const data=__deserializeClone(JSON.stringify({t:'view',id:1,c:'DataView',b:'AQIDBA==',o:1,l:2}));
        if(data.byteOffset!==1||data.byteLength!==2||data.getUint8(1)!==3)
            throw Error('legacy data view envelope');
        postMessage(true);
    "#,
    );
}
