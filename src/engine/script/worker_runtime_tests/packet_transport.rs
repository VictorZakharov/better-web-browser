//! Actual Window → worker → Window binary delivery, not wire-format feature probes.
use super::*;
use crate::engine::script::worker_message::WorkerMessage;

fn worker(source: &str) -> (WorkerRuntime, WorkerRuntimeOutcome) {
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/worker.js",
        source,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (runtime.expect("worker initializes"), outcome)
}
fn assert_clean(outcome: &WorkerRuntimeOutcome) {
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        !outcome
            .console
            .iter()
            .any(|line| line.starts_with("error:")),
        "{:?}",
        outcome.console
    );
}
fn outgoing(source: &str) -> Vec<WorkerMessage> {
    let (_, outcome) =
        crate::engine::script::tests::execute_html(&format!("<script>{source}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome
        .worker_actions
        .into_iter()
        .filter_map(|action| match action {
            ScriptWorkerAction::PostMessage { serialized, .. } => Some(serialized),
            _ => None,
        })
        .collect()
}

#[test]
fn window_worker_packets_keep_graph_aliases_offsets_cycles_and_sender_isolation() {
    let mut messages = outgoing(
        r#"
        const worker=new Worker('/worker.js');
        const buffer=new ArrayBuffer(48), bytes=new Uint8Array(buffer,5,12), data=new DataView(buffer,7,6);
        bytes[2]=91;bytes[11]=255;
        const graph={buffer,bytes,data,again:bytes};graph.self=graph;
        worker.postMessage(graph);
        bytes[2]=17;
    "#,
    );
    let packet = messages.pop().unwrap();
    assert_eq!(
        packet.binary_count(),
        1,
        "one binary per backing store, not per view"
    );
    assert!(
        packet.as_str().len() < 1024,
        "binary data is not encoded into metadata"
    );
    let (mut runtime, _) = worker(
        r#"
        onmessage=e=>{
            const x=e.data;
            if(x.self!==x||x.again!==x.bytes||x.bytes.buffer!==x.buffer||
                x.data.buffer!==x.buffer||x.bytes.byteOffset!==5||x.data.byteOffset!==7||
                x.bytes[2]!==91||x.bytes[11]!==255) throw Error('graph contract');
            x.data.setUint8(0,73);
            if(x.bytes[2]!==73) throw Error('receiver aliases');
            postMessage('passed');
        };
    "#,
    );
    let result = runtime.dispatch_packet(packet);
    assert_clean(&result);
    assert_eq!(result.messages, ["\"passed\""]);
}

#[test]
fn transfer_detaches_only_after_graph_getters_and_keeps_worker_return_bytes() {
    let messages = outgoing(
        r#"
        const worker=new Worker('/worker.js');
        const bytes=new Uint8Array([1,2,3,4]);
        worker.postMessage({bytes,get changed(){bytes[0]=93;return bytes.byteLength;}},[bytes.buffer]);
        if(bytes.byteLength!==0)throw Error('transfer did not detach');
    "#,
    );
    let (mut runtime, _) = worker(
        r#"
        onmessage=e=>{
            if(e.data.changed!==4||e.data.bytes[0]!==93)throw Error('snapshot ordering');
            postMessage(e.data.bytes,[e.data.bytes.buffer]);
            if(e.data.bytes.byteLength!==0)throw Error('worker return transfer');
        };
    "#,
    );
    let result = runtime.dispatch_packet(messages[0].clone());
    assert_clean(&result);
    assert_eq!(result.messages.len(), 1);
    let response = result.messages[0].clone();
    assert_eq!(response.binary_count(), 1);
    let (mut sink, _) = worker(r#"onmessage=e=>postMessage([...e.data].join(','));"#);
    let result = sink.dispatch_packet(response);
    assert_clean(&result);
    assert_eq!(result.messages, ["\"93,2,3,4\""]);
}

#[test]
fn reentrant_getters_can_post_nested_packets_and_clone_persistent_records() {
    let messages = outgoing(
        r#"
        const worker=new Worker('/worker.js'),bytes=new Uint8Array([11,12]);
        worker.postMessage({bytes,get nested(){
            worker.postMessage(new Uint8Array([71,72]));
            const copy=structuredClone(new Uint8Array([31,32]));
            return copy[0];
        }});
    "#,
    );
    assert_eq!(messages.len(), 2);
    let (mut runtime, _) = worker(
        r#"
        onmessage=e=>postMessage(e.data instanceof Uint8Array?
            [...e.data].join(','):e.data.bytes[0]+':'+e.data.nested);
    "#,
    );
    let inner = runtime.dispatch_packet(messages[0].clone());
    assert_clean(&inner);
    assert_eq!(inner.messages, ["\"71,72\""]);
    let outer = runtime.dispatch_packet(messages[1].clone());
    assert_clean(&outer);
    assert_eq!(outer.messages, ["\"11:31\""]);
}

#[test]
fn binary_worker_messages_use_captured_serializer_and_decoder_not_author_replacements() {
    let messages = outgoing(
        r#"
        const worker=new Worker('/worker.js');
        for(const name of ['__serializeClone','__serializeWorkerPacket','__deserializeWorkerPacketWithPorts'])
            globalThis[name]=()=>{throw Error('author replacement '+name);};
        btoa=atob=()=>{throw Error('author codec');};
        worker.postMessage(new Uint8Array([19,23]));
    "#,
    );
    let (mut runtime, _) = worker(
        r#"
        for(const name of ['__serializeClone','__deserializeCloneWithPorts','__dispatchWorkerMessage'])
            globalThis[name]=()=>{throw Error('author replacement '+name);};
        btoa=atob=()=>{throw Error('author codec');};
        onmessage=e=>postMessage(e.data[0]+e.data[1]);
    "#,
    );
    let result = runtime.dispatch_packet(messages[0].clone());
    assert_clean(&result);
    assert_eq!(result.messages, ["42"]);
}

#[test]
fn failed_getters_do_not_leave_a_writer_or_detach_unreached_transfer_steps() {
    let messages = outgoing(
        r#"
        const worker=new Worker('/worker.js'),buffer=new ArrayBuffer(8);
        let caught=false;
        try{worker.postMessage({buffer,get bad(){throw Error('getter failed');}},[buffer]);}
        catch(e){caught=e.message==='getter failed';}
        if(!caught||buffer.byteLength!==8)throw Error('failure transfer contract');
        worker.postMessage(new Uint8Array([97]));
    "#,
    );
    assert_eq!(messages.len(), 1);
    let (mut runtime, _) = worker("onmessage=e=>postMessage(e.data[0]);");
    let result = runtime.dispatch_packet(messages[0].clone());
    assert_clean(&result);
    assert_eq!(result.messages, ["97"]);
}

#[test]
fn texture_sized_packets_are_bounded_without_base64_expansion() {
    let (_, outcome) = worker(
        r#"
        const size=4*1024*1024;
        const maps=[new Uint8Array(size),new Uint8Array(size),new Uint8Array(size)];
        maps.forEach((x,i)=>{x[0]=i+1;x[size-1]=255-i;});
        postMessage(maps,maps.map(x=>x.buffer));
        if(maps.some(x=>x.byteLength!==0))throw Error('texture sender detach');
    "#,
    );
    assert_eq!(outcome.messages.len(), 1);
    let packet = outcome.messages[0].clone();
    assert_eq!(packet.binary_count(), 3);
    assert!(packet.as_str().len() < 1024);
    assert!(packet.capacity() < 12 * 1024 * 1024 + 4096);
    let (mut runtime, _) = worker(
        r#"
        onmessage=e=>{
            e.data.forEach((x,i)=>{if(x.length!==4*1024*1024||x[0]!==i+1||x[x.length-1]!==255-i)
                throw Error('texture receiver bytes');});postMessage(true);
        };
    "#,
    );
    let result = runtime.dispatch_packet(packet);
    assert_clean(&result);
    assert_eq!(result.messages, ["true"]);
}

#[test]
fn module_top_level_await_retains_binary_envelopes_until_dispatch_in_order() {
    let messages = outgoing(
        r#"
        const worker=new Worker('/worker.js');
        worker.postMessage(new Uint8Array([17,19]));
        worker.postMessage(new Uint8Array([71,73]));
    "#,
    );
    let (runtime, started) = WorkerRuntime::start(
        "https://example.com/worker.js",
        r#"
        await new Promise(resolve=>setTimeout(resolve,10));
        onmessage=e=>postMessage(e.data[0]+e.data[1]);
    "#,
        "",
        ScriptKind::Module,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert_clean(&started);
    let mut runtime = runtime.unwrap();
    for message in messages {
        let queued = runtime.dispatch_packet(message);
        assert_clean(&queued);
        assert!(queued.messages.is_empty());
    }
    let settled = runtime.advance_time(Duration::from_millis(10), 8);
    assert_clean(&settled);
    assert_eq!(settled.messages, ["36", "144"]);
}

#[test]
fn oversized_packet_fails_as_data_clone_error_and_unwinds_before_next_post() {
    let (_, outcome) = worker(
        r#"
        const a=new Uint8Array(16*1024*1024),b=new Uint8Array(16*1024*1024);
        let failure;
        try{postMessage([a,b]);}catch(e){failure=e.name;}
        if(failure!=='DataCloneError'||a.byteLength!==16*1024*1024||b.byteLength!==16*1024*1024)
            throw Error('bounded clone failure');
        postMessage(new Uint8Array([97]));
    "#,
    );
    assert_eq!(outcome.messages.len(), 1);
    let (mut sink, _) = worker("onmessage=e=>postMessage(e.data[0]);");
    let result = sink.dispatch_packet(outcome.messages[0].clone());
    assert_clean(&result);
    assert_eq!(result.messages, ["97"]);
}
