use super::*;

#[test]
fn opaque_child_can_message_but_cannot_read_or_mutate_its_parent() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.received = [];
        addEventListener('message', event => received.push([event.data, event.origin, event.source === f.contentWindow]));
        const f = document.createElement('iframe'); f.sandbox = 'allow-scripts';
        f.srcdoc = `<script>
            const blocked = operation => { try { operation(); return false; } catch (e) { return e.name === 'SecurityError' && e instanceof DOMException; } };
            const checks = [
                blocked(() => parent.document), blocked(() => parent.secret),
                blocked(() => parent.secret = 1), blocked(() => delete parent.postMessage),
                blocked(() => Object.defineProperty(parent, 'postMessage', {value: 1})),
                Object.getPrototypeOf(parent) === null,
                blocked(() => parent.location.href), blocked(() => parent.location.assign),
                blocked(() => parent.location = '/escape'),
                parent.window === parent && parent.self === parent && parent.frames === parent,
                parent.top === parent && parent.parent === parent,
                parent.postMessage.constructor('return globalThis')() === globalThis,
                Object.getOwnPropertyDescriptor(parent, 'location').get.constructor('return globalThis')() === globalThis,
                parent.postMessage === parent.postMessage, parent.then === undefined,
                Reflect.ownKeys(parent).includes('postMessage'), !Reflect.ownKeys(parent).includes('document')
            ];
            parent.postMessage(checks, '*');
        <\/script>`;
        document.body.append(f);
    </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(received.length !== 1 || received[0][0].some(v => !v) || received[0][1] !== 'null' || !received[0][2]) throw Error(JSON.stringify(received));",
    );
}

#[test]
fn cross_origin_relay_receives_caller_realm_clones_and_honors_target_origin() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.received = [];
        addEventListener('message', event => received.push([event.data, event.origin, event.source === f.contentWindow]));
        const f = document.createElement('iframe'); f.src = 'https://other.test/frame'; document.body.append(f);
    </script>"#,
    );
    let id = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    let result = runtime.complete_fetch_with_loader(id, Ok(response("https://other.test/frame", r#"<script>
        addEventListener('message', event => {
            const valid = event.origin === 'https://example.com' && event.source === parent && event.data instanceof Map;
            event.source.postMessage(valid && event.data.get('value') === 42, event.origin);
        });
    </script>"#)), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentDocument !== null) throw Error('foreign DOM exposed'); f.contentWindow.postMessage(new Map([['value', 42]]), 'https://wrong.test'); f.contentWindow.postMessage(new Map([['value', 42]]), 'https://other.test');",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(received.length !== 1 || received[0].join(',') !== 'true,https://other.test,true') throw Error(JSON.stringify(received));",
    );
}

#[test]
fn same_origin_sandbox_cannot_navigate_parent_through_borrowed_functions() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.result = [];
        const f = document.createElement('iframe'); f.sandbox = 'allow-same-origin allow-scripts';
        f.srcdoc = `<script>
            for (const run of [() => parent.location.href = '/escape', () => parent.location.assign('/escape'),
                () => parent.location.replace('/escape'), () => parent.__hostCall('navigate', '/escape')]) {
                try { run(); parent.result.push(false); } catch(e) { parent.result.push(e.name === 'SecurityError'); }
            }
        <\/script>`;
        document.body.append(f);
    </script>"#,
    );
    for _ in 0..40 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(
            result.navigation_url.is_none(),
            "sandbox escaped: {:?}",
            result.navigation_url
        );
    }
    evaluate(
        &mut runtime,
        &dom,
        "if(result.length !== 4 || result.some(v=>!v)) throw Error(JSON.stringify(result));",
    );
}
