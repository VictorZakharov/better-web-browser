//! Private Canvas records must not pass through mutable author iterators,
//! descriptor inheritance or serialization hooks.
use super::*;

fn check(source: &str) {
    let wire = include_str!("../bootstrap/canvas_private_wire.js");
    let (_, outcome) = execute_html(&format!("<script>(()=>{{{wire}\n{source}}})();</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/private-canvas-wire.js",
        &format!("(()=>{{{wire}\n{source}}})();postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}

#[test]
fn copied_key_lists_cannot_be_rewritten_by_an_author_array_iterator() {
    check(
        r#"
        const value={path:[{points:[[1,2],[3,4]],closed:false}],color:[1,2,3,255]};
        const expected=JSON.stringify(value), original=Array.prototype[Symbol.iterator];
        let iterated=0, observed=0, actual;
        Object.defineProperty(Object.prototype,'toJSON',{configurable:true,get(){observed++;throw Error('private receiver escaped')}});
        Array.prototype[Symbol.iterator]=function*(){iterated++;yield 'toJSON'};
        try {actual=canvasPrivateWireStringify(value)}
        finally {Array.prototype[Symbol.iterator]=original;delete Object.prototype.toJSON}
        if(iterated || observed || actual!==expected)throw Error('mutable key iterator reached private records');
    "#,
    );
}

#[test]
fn inherited_descriptor_accessors_do_not_run_during_owned_snapshot_creation() {
    check(
        r#"
        const value={kind:'fill',transform:[1,0,0,1,8,9],rule:'evenodd'};
        const expected=JSON.stringify(value);
        let observed=0,actual;
        Object.defineProperty(Object.prototype,'get',{configurable:true,get(){observed++;throw Error('inherited descriptor accessor')}});
        try {actual=canvasPrivateWireStringify(value)} finally {delete Object.prototype.get}
        if(observed || actual!==expected)throw Error('descriptor prototype affected a private snapshot');
    "#,
    );
}

#[test]
fn accessor_records_are_rejected_without_invoking_their_getters() {
    check(
        r#"
        let observed=0;
        const value=Object.create(null);
        Object.defineProperty(value,'path',{enumerable:true,get(){observed++;return []}});
        let rejected=false;
        try {canvasPrivateWireStringify(value)} catch(error){rejected=error.name==='NotSupportedError'}
        if(!rejected || observed)throw Error('private wire evaluated an accessor');
    "#,
    );
}
