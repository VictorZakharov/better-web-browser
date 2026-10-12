use super::webgl2_bindings_tests::check;
use super::*;
use crate::engine::script::{
    HostState, ScriptKind, module_loader::WebModuleLoader, worker_host::WorkerHostState,
};
use std::sync::Arc;

// Exercise the private native bridge before bootstrap removes its global name.
// Public Window/Worker painting tests separately cover the captured call site.
fn both(code: &str) {
    let document = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let worker = Rc::new(RefCell::new(WorkerHostState::new(
        "https://example.test/worker.js",
        true,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
    )));
    for bridge in [
        HostBridge::Document(Rc::downgrade(&document)),
        HostBridge::Worker(Rc::downgrade(&worker)),
    ] {
        let mut context = Context::new(bridge).unwrap();
        check(&mut context, code);
    }
}

#[test]
fn bitmap_predicate_reads_only_admitted_view_rows_and_never_changes_their_storage() {
    both(
        r#"
        const inspect=value=>__hostCall('canvasBitmapHasHiddenRgb',value);
        if(inspect(new Uint8ClampedArray([0,0,0,0]))!==false||
            inspect(new Uint8Array([1,0,0,0]))!==true||inspect(new Uint8Array([1,0,0,1]))!==false)
            throw Error('bitmap predicate channels');
        const backing=new Uint8Array([1,0,0,0,19,37,71,255,1,0,0,0]);
        const view=new Uint8Array(backing.buffer,4,4);
        if(inspect(view)!==false||String(backing)!=='1,0,0,0,19,37,71,255,1,0,0,0')throw Error('view offset/ownership');
        view[3]=0;
        if(inspect(view)!==true)throw Error('call-time bitmap state');
        const typed=Object.getPrototypeOf(Uint8Array.prototype),descriptors=Object.getOwnPropertyDescriptors(typed);
        let callbacks=0;
        try{
            for(const name of ['buffer','byteOffset','byteLength','length'])
                Object.defineProperty(typed,name,{configurable:true,get(){callbacks++;throw Error('author pixel getter')}});
            if(inspect(view)!==true)throw Error('intrinsic bitmap admission');
        }finally{Object.defineProperties(typed,descriptors);}
        if(callbacks)throw Error('predicate invoked author getter');
    "#,
    );
}

#[test]
fn bitmap_predicate_rejects_detached_shared_resizable_oversized_and_forged_inputs() {
    both(
        r#"
        const inspect=value=>__hostCall('canvasBitmapHasHiddenRgb',value);
        const detached=new Uint8Array(4);detached.buffer.transfer();
        const values=[null,{},new Uint16Array(2),new DataView(new ArrayBuffer(4)),
            new Uint8Array(3),detached,new Uint8Array(new SharedArrayBuffer(4)),
            new Uint8Array(new ArrayBuffer(4,{maxByteLength:8})),new Uint8Array(16*1024*1024+4),
            new Proxy(new Uint8Array(4),{get(){throw Error('proxy trap')}})];
        for(const value of values)if(inspect(value)!==null)throw Error('unsafe bitmap admitted');
        if(inspect(new Uint8Array(0))!==false)throw Error('empty fixed bitmap');
    "#,
    );
}
