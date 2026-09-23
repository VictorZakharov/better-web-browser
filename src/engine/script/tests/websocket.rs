use super::*;
use crate::renderer_protocol::{
    DocumentId, WebSocketEvent, WebSocketEventKind, WebSocketOperation,
};

fn socket_event(id: u32, kind: WebSocketEventKind) -> WebSocketEvent {
    WebSocketEvent {
        document: DocumentId::new(1).unwrap(),
        socket_id: u64::from(id),
        kind,
    }
}

#[test]
fn websocket_constructor_exposes_standard_state_and_validates_before_ipc() {
    let (dom, outcome) = execute_html(
        r#"<body><div></div><script>
        const ws = new WebSocket('https://echo.example.test/socket', ['chat', 'chat.v2']);
        let duplicate = '';
        try { new WebSocket('/other', ['chat', 'chat']); } catch (error) { duplicate = error.name; }
        document.querySelector('div').textContent = [
            ws instanceof EventTarget, ws.url, ws.readyState, WebSocket.CONNECTING,
            ws.binaryType, duplicate, Object.prototype.toString.call(ws)
        ].join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|wss://echo.example.test/socket|0|0|blob|SyntaxError|[object WebSocket]"
    );
    assert_eq!(outcome.websocket_actions.len(), 1);
    assert!(matches!(
        &outcome.websocket_actions[0].operation,
        WebSocketOperation::Open { url, protocols }
            if url == "wss://echo.example.test/socket"
                && protocols == &["chat", "chat.v2"]
    ));
}

#[test]
fn websocket_open_message_send_and_close_are_delivered_as_events() {
    let dom = dom::parse_with_scripting(
        r#"<body><div></div><script>
        const ws = new WebSocket('wss://echo.example.test/socket');
        const seen = [];
        ws.onopen = event => {
            seen.push('open:' + ws.readyState + ':' + event.isTrusted);
            ws.send('hello');
        };
        ws.binaryType = 'arraybuffer';
        ws.onmessage = event => seen.push('message:' + event.data.byteLength + ':' + event.isTrusted);
        ws.onclose = event => {
            seen.push('close:' + event.code + ':' + event.wasClean + ':' + ws.readyState);
            document.querySelector('div').textContent = seen.join('|');
        };
        </script></body>"#,
        true,
    );
    let script = ScriptInput {
        source_url: "https://example.com/#inline".into(),
        code: dom.elements_named("script").next().unwrap().text_content(),
        node: dom.elements_named("script").next().unwrap(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[script]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let id = initial.websocket_actions[0].id;
    let opened = runtime.deliver_websocket_event(socket_event(
        id,
        WebSocketEventKind::Open {
            protocol: String::new(),
        },
    ));
    assert!(opened.errors.is_empty(), "{:?}", opened.errors);
    assert!(matches!(
        &opened.websocket_actions[0].operation,
        WebSocketOperation::Send { binary: false, data } if data == b"hello"
    ));
    let message = runtime.deliver_websocket_event(socket_event(
        id,
        WebSocketEventKind::Message {
            binary: true,
            data: vec![0, 255, 42],
        },
    ));
    assert!(message.errors.is_empty(), "{:?}", message.errors);
    let closed = runtime.deliver_websocket_event(socket_event(
        id,
        WebSocketEventKind::Close {
            code: 1000,
            reason: String::new(),
            clean: true,
        },
    ));
    assert!(closed.errors.is_empty(), "{:?}", closed.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "open:1:true|message:3:true|close:1000:true:3"
    );
}
