use super::*;

#[test]
fn message_channel_queues_cloned_messages_until_the_port_starts() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><script>
            const channel = new MessageChannel();
            const original = { value: 42 };
            let synchronous = true;
            channel.port1.postMessage(original);
            channel.port2.onmessage = event => {
                const accepted = !synchronous && event.data !== original && event.data.value === 42 &&
                    event.origin === '' && event.source === null && event.ports.length === 0 &&
                    event.target === channel.port2 && channel.port1 instanceof MessagePort;
                if (accepted) document.querySelector('output').textContent = 'yes';
            };
            synchronous = false;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn transferred_port_keeps_its_entanglement_and_detaches_the_sender_wrapper() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><script>
            const transport = new MessageChannel();
            const nested = new MessageChannel();
            let detached = false;
            transport.port2.onmessage = event => {
                const received = event.ports[0];
                if (received instanceof MessagePort && event.data === 'port') received.postMessage('pong');
            };
            nested.port1.onmessage = event => {
                if (event.data === 'pong' && detached)
                    document.querySelector('output').textContent = 'yes';
            };
            transport.port1.postMessage('port', [nested.port2]);
            try { nested.port2.postMessage('discarded', [nested.port2]); }
            catch (error) { detached = error instanceof DOMException && error.name === 'DataCloneError'; }
            // A detached port is inert; attempting to transfer it demonstrates detachment.
            if (!detached) {
                try { transport.port1.postMessage('again', [nested.port2]); }
                catch (error) { detached = error instanceof DOMException && error.name === 'DataCloneError'; }
            }
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
