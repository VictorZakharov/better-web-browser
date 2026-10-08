//! Exercise the actual private encoder in both bootstraps, without exporting
//! it to author code or replacing the production host bridge.
use super::*;

fn check(source: &str) {
    let encoder = include_str!("../bootstrap/canvas_packed_geometry.js");
    let script = format!("(()=>{{{encoder}\n{source}}})();");
    let (_, outcome) = execute_html(&format!("<script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/packed-canvas.js",
        &format!("{script}postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}

#[test]
fn private_encoder_preserves_f64_coordinates_signed_zero_and_independent_snapshots() {
    check(
        r#"
        const path={subpaths:[{closed:false,points:[[.5,-0],[1/3,7.125]]},{closed:true,points:[]}]};
        const a=canvasPackGeometry(path), view=new DataView(a.buffer);
        if(a.byteLength!==56 || view.getUint32(0,true)!==0x31475043 || view.getUint32(4,true)!==2)
            throw Error('version/size/parts');
        if(view.getUint32(8,true)!==2 || view.getUint32(12,true)!==0 ||
            view.getFloat64(16,true)!==.5 || !Object.is(view.getFloat64(24,true),-0) ||
            view.getFloat64(32,true)!==1/3 || view.getUint32(52,true)!==1) throw Error('coordinates/flags');
        path.subpaths[0].points[0][0]=23;
        const b=canvasPackGeometry(path);
        if(new DataView(b.buffer).getFloat64(16,true)!==23 || view.getFloat64(16,true)!==.5)
            throw Error('geometry aliases its source or another request');
        if(canvasPackGeometry({subpaths:[]}).byteLength!==8) throw Error('empty geometry');
    "#,
    );
}

#[test]
fn private_encoder_uses_captured_buffer_accessors_and_numeric_writers() {
    check(
        r#"
        const path={subpaths:[{closed:true,points:[[1,2],[3,4],[5,6]]}]};
        const prototype=Object.getPrototypeOf(Uint8Array.prototype);
        const descriptor=Object.getOwnPropertyDescriptor(prototype,'buffer');
        const word=DataView.prototype.setUint32,number=DataView.prototype.setFloat64;
        let calls=0,bytes;
        Object.defineProperty(prototype,'buffer',{configurable:true,get(){calls++;throw Error('private buffer escaped')}});
        DataView.prototype.setUint32=DataView.prototype.setFloat64=()=>{calls++;throw Error('private view escaped')};
        try {bytes=canvasPackGeometry(path)} finally {
            Object.defineProperty(prototype,'buffer',descriptor);
            DataView.prototype.setUint32=word;DataView.prototype.setFloat64=number;
        }
        if(calls || new DataView(bytes.buffer).getFloat64(16,true)!==1) throw Error('mutable intrinsic ran');
    "#,
    );
}

#[test]
fn private_encoder_applies_aggregate_points_and_subpath_budgets_before_allocation() {
    check(
        r#"
        const points=Array.from({length:4096},()=>[0,0]);
        const path={subpaths:[{closed:false,points},{closed:true,points}]};
        if(canvasPackGeometry(path).byteLength!==8+16+8192*16) throw Error('exact point budget rejected');
        path.subpaths.push({closed:false,points:[[0,0]]});
        if(canvasPackGeometry(path)!==null) throw Error('aggregate point budget ignored');
        const empty=Array.from({length:8192},()=>({closed:false,points:[]}));
        if(canvasPackGeometry({subpaths:empty}).byteLength!==8+8192*8) throw Error('exact part budget');
        empty.push({closed:false,points:[]});
        if(canvasPackGeometry({subpaths:empty})!==null) throw Error('part budget ignored');
    "#,
    );
}
