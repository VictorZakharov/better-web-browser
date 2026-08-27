use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

#[test]
fn open_and_closed_roots_preserve_tree_scope_and_shadow_including_connectivity() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="open"></div><x-closed id="closed"></x-closed><script>
            const openHost = document.getElementById('open');
            const open = openHost.attachShadow({ mode: 'open', delegatesFocus: true, serializable: true });
            open.innerHTML = '<section id="inside"><span>content</span></section>';
            const closedHost = document.getElementById('closed');
            const closed = closedHost.attachShadow({ mode: 'closed' });
            closed.innerHTML = '<p id="secret">secret</p>';
            const inside = open.getElementById('inside');
            const valid = open instanceof ShadowRoot && open instanceof DocumentFragment &&
                openHost.shadowRoot === open && closedHost.shadowRoot === null && closed.host === closedHost &&
                open.mode === 'open' && closed.mode === 'closed' && open.delegatesFocus && open.serializable &&
                inside.parentNode === open && inside.getRootNode() === open &&
                inside.getRootNode({ composed: true }) === document && inside.isConnected &&
                document.getElementById('inside') === null && document.querySelector('#secret') === null;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn named_and_default_slots_update_assignment_and_coalesce_slotchange() {
    let (dom, outcome) = execute_html(
        r#"<body><x-card id="host"><h1 slot="title">Title</h1><p>Body</p></x-card><script>
            const host = document.getElementById('host');
            const root = host.attachShadow({ mode: 'open' });
            root.innerHTML = '<header><slot name="title"></slot></header><main><slot>fallback</slot></main>';
            const slots = root.querySelectorAll('slot');
            const changes = [];
            slots.forEach(slot => slot.addEventListener('slotchange', () => changes.push(slot.name || 'default')));
            const title = host.children[0];
            const body = host.children[1];
            const initial = slots[0].assignedElements()[0] === title &&
                slots[1].assignedNodes()[0] === body && title.assignedSlot === slots[0] &&
                body.assignedSlot === slots[1];
            title.slot = '';
            title.slot = 'title';
            queueMicrotask(() => {
                const valid = initial && slots[0].assignedNodes({ flatten: true })[0] === title &&
                    slots[1].assignedElements().length === 1 &&
                    changes.filter(name => name === 'title').length === 1 &&
                    changes.filter(name => name === 'default').length === 1;
                document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + changes.join(','));
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn attach_shadow_rejects_invalid_hosts_modes_and_duplicate_roots() {
    let (dom, outcome) = execute_html(
        r#"<body><button id="button"></button><div id="host"></div><script>
            const failures = [];
            const expect = (name, callback, expected) => {
                try { callback(); failures.push(name + ':missing'); }
                catch (error) { if (error.name !== expected) failures.push(name + ':' + error.name); }
            };
            expect('invalid-host', () => document.getElementById('button').attachShadow({ mode: 'open' }), 'NotSupportedError');
            expect('invalid-mode', () => document.createElement('div').attachShadow({ mode: 'private' }), 'TypeError');
            expect('manual-slots', () => document.createElement('div').attachShadow({ mode: 'open', slotAssignment: 'manual' }), 'NotSupportedError');
            expect('constructor', () => new ShadowRoot(), 'TypeError');
            const host = document.getElementById('host');
            host.attachShadow({ mode: 'open' });
            expect('duplicate', () => host.attachShadow({ mode: 'open' }), 'NotSupportedError');
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn composed_events_cross_and_retarget_while_non_composed_events_stop_at_the_root() {
    let (dom, outcome) = execute_html(
        r#"<body><x-events id="host"></x-events><script>
            const host = document.getElementById('host');
            const root = host.attachShadow({ mode: 'open' });
            root.innerHTML = '<button id="target">go</button>';
            const target = root.querySelector('#target');
            const seen = [];
            root.addEventListener('probe', event => seen.push('root:' + event.target.id + ':' +
                event.composedPath().map(node => node.id || node.nodeName).join('>')));
            host.addEventListener('probe', event => seen.push('host:' + event.target.id + ':' +
                event.composedPath().map(node => node.id || node.nodeName).join('>')));
            document.body.addEventListener('probe', event => seen.push('body:' + event.target.id));
            target.dispatchEvent(new Event('probe', { bubbles: true, composed: false }));
            target.dispatchEvent(new Event('probe', { bubbles: true, composed: true }));
            const valid = seen.length === 4 && seen[0].startsWith('root:target:target>#document-fragment') &&
                seen[1].startsWith('root:target:target>#document-fragment>host') &&
                seen[2].startsWith('host:host:target>#document-fragment>host') && seen[3] === 'body:host';
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + JSON.stringify(seen));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn closed_roots_hide_internal_nodes_from_external_composed_paths() {
    let (dom, outcome) = execute_html(
        r#"<body><x-closed-events id="host"></x-closed-events><script>
            const host = document.getElementById('host');
            const root = host.attachShadow({ mode: 'closed' });
            root.innerHTML = '<button id="secret">go</button>';
            const secret = root.querySelector('#secret');
            let inside = '';
            let outside = '';
            root.addEventListener('probe', event => {
                inside = event.target.id + ':' + event.composedPath().some(node => node === secret);
            });
            host.addEventListener('probe', event => {
                outside = event.target.id + ':' + event.composedPath().some(node => node === secret) + ':' +
                    event.composedPath().some(node => node === root);
            });
            secret.dispatchEvent(new Event('probe', { bubbles: true, composed: true }));
            document.body.setAttribute('data-result', inside === 'secret:true' && outside === 'host:false:false'
                ? 'pass' : 'fail:' + inside + '/' + outside);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn closed_roots_hide_assigned_slots_but_preserve_shadow_including_custom_element_lifecycle() {
    let (dom, outcome) = execute_html(
        r#"<body><x-shell id="host"><span id="light">light</span></x-shell><script>
            const calls = [];
            class XInside extends HTMLElement {
                connectedCallback() { calls.push('connected'); }
                disconnectedCallback() { calls.push('disconnected'); }
            }
            customElements.define('x-inside', XInside);
            const host = document.getElementById('host');
            const light = document.getElementById('light');
            const root = host.attachShadow({ mode: 'closed' });
            root.innerHTML = '<slot></slot><x-inside id="inside"></x-inside>';
            const inside = root.getElementById('inside');
            const privateAssignment = light.assignedSlot === null;
            host.remove();
            document.body.appendChild(host);
            const valid = privateAssignment && inside.isConnected &&
                calls.join(',') === 'connected,disconnected,connected';
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + calls.join(','));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
