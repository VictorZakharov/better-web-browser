use super::*;
use crate::renderer_protocol::{
    DocumentId, WebSocketEvent, WebSocketEventKind, WebSocketOperation,
};

fn event(id: u32, kind: WebSocketEventKind) -> WebSocketEvent {
    WebSocketEvent {
        document: DocumentId::new(1).unwrap(),
        socket_id: u64::from(id),
        kind,
    }
}

#[test]
fn dedicated_worker_websocket_uses_its_url_and_delivers_ordered_events() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, initial) = WorkerRuntime::start(
        "https://worker.example.test/scripts/entry.js",
        r#"const socket = new WebSocket('/echo', 'chat');
           socket.binaryType = 'arraybuffer';
           socket.onopen = event => {
               postMessage('open:' + event.isTrusted + ':' + socket.protocol);
               socket.send('hello');
           };
           socket.onmessage = event => {
               postMessage('message:' + event.isTrusted + ':' +
                   String(new Uint8Array(event.data)));
               socket.close(1000);
           };
           socket.onclose = event => postMessage('close:' + event.code + ':' + event.wasClean);"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let [action] = initial.websocket_actions.as_slice() else {
        panic!("expected one Worker WebSocket open")
    };
    assert!(
        matches!(&action.operation, WebSocketOperation::Open { url, protocols }
        if url == "wss://worker.example.test/echo" && protocols == &["chat"])
    );
    let id = action.id;
    let mut runtime = runtime.unwrap();
    let opened = runtime.deliver_websocket_event(event(
        id,
        WebSocketEventKind::Open {
            protocol: "chat".into(),
        },
    ));
    assert!(opened.errors.is_empty(), "{:?}", opened.errors);
    assert_eq!(opened.messages, ["\"open:true:chat\""]);
    assert!(matches!(&opened.websocket_actions[0].operation,
        WebSocketOperation::Send { binary: false, data } if data == b"hello"));
    let received = runtime.deliver_websocket_event(event(
        id,
        WebSocketEventKind::Message {
            binary: true,
            data: vec![0, 255, 42],
        },
    ));
    assert!(received.errors.is_empty(), "{:?}", received.errors);
    assert_eq!(received.messages, ["\"message:true:0,255,42\""]);
    assert!(matches!(
        &received.websocket_actions[0].operation,
        WebSocketOperation::Close { code: 1000, .. }
    ));
    let closed = runtime.deliver_websocket_event(event(
        id,
        WebSocketEventKind::Close {
            code: 1000,
            reason: String::new(),
            clean: true,
        },
    ));
    assert!(closed.errors.is_empty(), "{:?}", closed.errors);
    assert_eq!(closed.messages, ["\"close:1000:true\""]);
}

#[test]
fn worker_websocket_rejects_duplicate_protocols_before_broker_dispatch() {
    let loader: Arc<WorkerSourceLoader> = Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/worker.js",
        r#"let name = '';
           try { new WebSocket('/echo', ['chat', 'chat']); }
           catch (error) { name = error.name; }
           postMessage(name);"#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.websocket_actions.is_empty());
    assert_eq!(outcome.messages, ["\"SyntaxError\""]);
}
