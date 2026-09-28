use super::*;
use crate::broadcast_channel::BroadcastRegistry;
use crate::renderer_protocol::{
    BroadcastCommand, BroadcastDelivery, BroadcastOperation, DocumentId,
};

#[test]
fn browser_registry_routes_between_distinct_document_realms() {
    let sender_dom = dom::parse_with_scripting(
        "<body><script>window.sender = new BroadcastChannel('room');</script></body>",
        true,
    );
    let receiver_dom = dom::parse_with_scripting(
        "<body><script>window.receiver = new BroadcastChannel('room'); receiver.onmessage = event => document.body.setAttribute('data-received', event.data);</script></body>",
        true,
    );
    let mut sender = ScriptRuntime::new(sender_dom.document.clone(), "https://example.com/one");
    let mut receiver = ScriptRuntime::new(receiver_dom.document.clone(), "https://example.com/two");
    let first = sender.execute_initial(&script_inputs(&sender_dom));
    let second = receiver.execute_initial(&script_inputs(&receiver_dom));
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    let mut registry = BroadcastRegistry::default();
    let origin = "https://example.com";
    for (tab, document, actions) in [
        (1, DocumentId::new(1).unwrap(), &first.broadcast_actions),
        (2, DocumentId::new(2).unwrap(), &second.broadcast_actions),
    ] {
        for action in actions {
            assert!(
                registry
                    .apply(
                        tab,
                        origin,
                        &BroadcastCommand {
                            document,
                            channel_id: action.channel_id,
                            operation: action.operation.clone(),
                        }
                    )
                    .unwrap()
            );
        }
    }
    let node = sender_dom.elements_named("script").next().unwrap();
    let posted = sender.execute_additional_with_loader(
        &[input(
            &node,
            "post.js",
            "sender.postMessage('cross-tab')",
            false,
        )],
        None,
    );
    assert!(posted.errors.is_empty(), "{:?}", posted.errors);
    for action in posted.broadcast_actions {
        assert!(
            registry
                .apply(
                    1,
                    origin,
                    &BroadcastCommand {
                        document: DocumentId::new(1).unwrap(),
                        channel_id: action.channel_id,
                        operation: action.operation,
                    }
                )
                .unwrap()
        );
    }
    assert!(!registry.has_pending(1));
    let mut delivery = None;
    assert!(registry.take_if(2, |next| {
        delivery = Some(next.clone());
        true
    }));
    let outcome = receiver.deliver_broadcast(&delivery.unwrap());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        receiver_dom
            .elements_named("body")
            .next()
            .unwrap()
            .attr("data-received")
            .as_deref(),
        Some("cross-tab")
    );
}

#[test]
fn broadcast_channel_clones_into_a_later_document_task() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            window.sender = new BroadcastChannel('\uD800');
            window.receiver = new BroadcastChannel('\uD800');
            receiver.onmessage = event => {
                const value = event.data;
                document.body.setAttribute('data-message', [
                    value.map.get('key'), value.date.getUTCFullYear(),
                    value.self === value, event.origin, event.source === null,
                    event.ports.length, event.isTrusted
                ].join('|'));
            };
            const value = { map: new Map([['key', 42]]), date: new Date('2020-01-01T00:00:00Z') };
            value.self = value;
            sender.postMessage(value);
            document.body.setAttribute('data-hook', typeof __dispatchBroadcastChannel);
            globalThis.__dispatchBroadcastChannel = () => { throw new Error('page spoof'); };
            globalThis.MessageEvent = () => { throw new Error('page constructor spoof'); };
            receiver.dispatchEvent = () => { throw new Error('page dispatch spoof'); };
            document.body.setAttribute('data-private', typeof __markTrustedEvent);
            document.body.setAttribute('data-before', String(document.body.hasAttribute('data-message')));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/path");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.broadcast_actions.len(), 3);
    assert!(matches!(
        &initial.broadcast_actions[0].operation,
        BroadcastOperation::Open { name } if name == "d800"
    ));
    assert!(matches!(
        &initial.broadcast_actions[1].operation,
        BroadcastOperation::Open { name } if name == "d800"
    ));
    let BroadcastOperation::Post { serialized } = &initial.broadcast_actions[2].operation else {
        panic!("expected structured-clone post");
    };
    let serialized = serialized.clone();
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-before").as_deref(), Some("false"));
    assert_eq!(body.attr("data-private").as_deref(), Some("undefined"));
    assert_eq!(body.attr("data-hook").as_deref(), Some("undefined"));
    assert!(body.attr("data-message").is_none());

    let delivered = runtime.deliver_broadcast(&BroadcastDelivery {
        document: DocumentId::new(1).unwrap(),
        channel_id: initial.broadcast_actions[1].channel_id,
        origin: "https://example.com".into(),
        serialized,
    });
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(
        body.attr("data-message").as_deref(),
        Some("42|2020|true|https://example.com|true|0|true")
    );
}

#[test]
fn broadcast_channel_close_and_clone_failure_are_observable() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const channel = new BroadcastChannel('closed');
            let cloneError = '';
            try { channel.postMessage(() => {}); } catch (error) { cloneError = error.name; }
            let reentrantError = '';
            try { channel.postMessage({ get value() { channel.close(); return 1; } }); }
            catch (error) { reentrantError = error.name; }
            channel.close();
            channel.close();
            let closedError = '';
            try { channel.postMessage(1); } catch (error) { closedError = error.name; }
            document.body.setAttribute('data-result', [cloneError, reentrantError, closedError].join('|'));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.broadcast_actions.len(), 2);
    assert!(matches!(
        outcome.broadcast_actions[1].operation,
        BroadcastOperation::Close
    ));
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("DataCloneError|InvalidStateError|InvalidStateError")
    );
}
