//! Real worker return delivery into a retained Window, including stale event retirement.
use super::*;
use crate::engine::script::{worker_message::WorkerMessage, worker_runtime::WorkerRuntime};

fn window(source: &str) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting(
        &format!("<body><div>pending</div><script>{source}</script>"),
        true,
    );
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    (dom, runtime)
}

fn packet(source: &str) -> WorkerMessage {
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages.len(), 1);
    outcome.messages.into_iter().next().unwrap()
}

fn text(dom: &dom::Dom) -> String {
    dom.elements_named("div").next().unwrap().text_content()
}

#[test]
fn binary_return_uses_private_window_dispatch_and_finishes_promise_jobs() {
    let (dom, mut runtime) = window(
        r#"
        const worker=new Worker('/worker.js');
        worker.onmessage=e=>{
            const x=e.data;
            if(!e.isTrusted||x.buffer!==x.view.buffer||x.self!==x||x.view[1]!==91)
                throw Error('Window clone/event contract');
            Promise.resolve().then(()=>document.querySelector('div').textContent=x.view.join(','));
        };
        globalThis.__completeWorkerEvent=()=>{throw Error('author replaced dispatch');};
        globalThis.__deserializeWorkerPacketWithPorts=()=>{throw Error('author replaced decoder');};
        atob=btoa=()=>{throw Error('author replaced codec');};
    "#,
    );
    let message = packet(
        r#"
        const buffer=new ArrayBuffer(16),view=new Uint8Array(buffer,3,4);
        view.set([17,91,93,255]);const graph={buffer,view};graph.self=graph;
        postMessage(graph,[buffer]);if(buffer.byteLength!==0)throw Error('return transfer');
    "#,
    );
    let result = runtime.complete_worker_event_with_loader(1, Ok(message), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.render_requested);
    assert_eq!(text(&dom), "17,91,93,255");
}

#[test]
fn each_delivery_gets_independent_mutable_receiver_storage() {
    let (dom, mut runtime) = window(
        r#"
        const worker=new Worker('/worker.js');let first;
        worker.onmessage=e=>{
            if(!first){first=e.data;first[0]=211;return;}
            document.querySelector('div').textContent=[first[0],e.data[0],first!==e.data,
                first.buffer!==e.data.buffer].join('|');
        };
    "#,
    );
    let message = packet("postMessage(new Uint8Array([71,72]));");
    for _ in 0..2 {
        let result = runtime.complete_worker_event_with_loader(1, Ok(message.clone()), None);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }
    assert_eq!(text(&dom), "211|71|true|true");
}

#[test]
fn malformed_binary_envelope_emits_messageerror_without_poisoning_next_delivery() {
    let (dom, mut runtime) = window(
        r#"
        const worker=new Worker('/worker.js');let failures=0;
        worker.onmessageerror=e=>{if(!e.isTrusted)throw Error('untrusted failure');failures++;};
        worker.onmessage=e=>document.querySelector('div').textContent=failures+':'+e.data[0];
    "#,
    );
    let result = runtime.complete_worker_event_with_loader(
        1,
        Ok(r#"{"t":"buffer","id":1,"v":"@breeze-binary/forged/0"}"#.into()),
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let result = runtime.complete_worker_event_with_loader(
        1,
        Ok(packet("postMessage(new Uint8Array([42]));")),
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(text(&dom), "1:42");
}

#[test]
fn terminated_worker_return_is_retired_without_author_dispatch() {
    let (dom, mut runtime) = window(
        r#"
        const worker=new Worker('/worker.js');
        worker.onmessage=()=>document.querySelector('div').textContent='stale';
        worker.terminate();
    "#,
    );
    let result = runtime.complete_worker_event_with_loader(
        1,
        Ok(packet("postMessage(new Uint8Array(1024*1024));")),
        None,
    );
    assert!(result.errors.is_empty());
    assert!(!result.render_requested);
    assert_eq!(text(&dom), "pending");
}

#[test]
fn terminate_in_getter_does_not_borrow_host_across_serialization() {
    let (_, outcome) = execute_html(
        r#"<script>
        const worker=new Worker('/worker.js'),bytes=new Uint8Array([1,2]);
        worker.postMessage({bytes,get end(){worker.terminate();return true;}},[bytes.buffer]);
        if(bytes.byteLength!==0)throw Error('transfer steps did not finish');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(matches!(
        outcome.worker_actions.as_slice(),
        [
            ScriptWorkerAction::Start { .. },
            ScriptWorkerAction::Terminate { .. }
        ]
    ));
}

#[test]
fn closing_during_worker_getter_discards_message_after_transfer_steps() {
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
        const bytes=new Uint8Array([1,2]);
        postMessage({bytes,get end(){close();return true;}},[bytes.buffer]);
        if(bytes.byteLength!==0)throw Error('closed sender transfer');
    "#,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.closed);
    assert!(outcome.messages.is_empty());
}
