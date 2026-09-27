use super::*;

fn result(markup: &str) -> String {
    let (dom, outcome) = execute_html(markup);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
        .unwrap_or_else(|| "missing result".into())
}

#[test]
fn parser_created_shadow_roots_assign_light_children_and_signal_initial_slots() {
    let actual = result(
        r#"<body>
        <div id=host><template shadowrootmode=open>
            <slot name=label></slot><slot></slot>
        </template><b slot=label id=named>named</b><i id=default>default</i></div>
        <script>
          const host = document.getElementById('host');
          const slots = host.shadowRoot.querySelectorAll('slot');
          const seen = [];
          slots.forEach((slot, index) => slot.addEventListener('slotchange', () => seen.push(index)));
          const assigned = slots[0].assignedNodes()[0] === document.getElementById('named') &&
              slots[1].assignedNodes()[0] === document.getElementById('default');
          queueMicrotask(() => document.body.setAttribute('data-result',
              assigned && seen.join(',') === '0,1' ? 'pass' : 'fail:' + assigned + ':' + seen));
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn prebuilt_fallback_does_not_signal_when_slot_is_inserted() {
    let actual = result(
        r#"<body><div id=imperative></div><script>
        const root = document.getElementById('imperative').attachShadow({mode:'open'});
        const prebuilt = document.createElement('slot');
        prebuilt.textContent = 'fallback';
        let changes = 0;
        prebuilt.addEventListener('slotchange', () => changes++);
        root.appendChild(prebuilt);
        queueMicrotask(() => document.body.setAttribute('data-result',
            changes === 0 ? 'pass' : 'fail:' + changes));
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn changed_then_restored_assignment_signals_once_after_mutation_observers() {
    let actual = result(
        r#"<body><div id=host></div><script>
        const host = document.getElementById('host');
        const root = host.attachShadow({mode:'open'});
        root.innerHTML = '<slot></slot>';
        queueMicrotask(() => {
            const slot = root.querySelector('slot'), order = [];
            new MutationObserver(records => order.push('observer:' + records.length))
                .observe(host, {childList:true});
            slot.addEventListener('slotchange', event =>
                order.push('slot:' + event.bubbles + ':' + event.composed));
            root.addEventListener('slotchange', () => order.push('root'));
            host.addEventListener('slotchange', () => order.push('escaped'));
            const light = document.createElement('b');
            host.appendChild(light);
            host.removeChild(light);
            queueMicrotask(() => document.body.setAttribute('data-result', order.join(',')));
        });
        </script>"#,
    );
    assert_eq!(actual, "observer:2,slot:true:false,root");
}

#[test]
fn declarative_manual_slots_and_fallback_changes_coalesce_per_checkpoint() {
    let actual = result(
        r#"<body><div id=host><template shadowrootmode=open shadowrootslotassignment=manual>
            <slot>initial</slot>
        </template><b id=light>light</b></div><script>
        const host = document.getElementById('host'), root = host.shadowRoot;
        const slot = root.querySelector('slot'), light = document.getElementById('light');
        const initiallyEmpty = root.slotAssignment === 'manual' && slot.assignedNodes().length === 0;
        queueMicrotask(() => {
            let changes = 0;
            slot.addEventListener('slotchange', () => changes++);
            slot.assign(light);
            slot.assign();
            slot.assign(light);
            const assigned = slot.assignedNodes()[0] === light;
            queueMicrotask(() => {
                slot.assign();
                const fallback = document.createTextNode('later');
                slot.appendChild(fallback);
                slot.removeChild(fallback);
                queueMicrotask(() => document.body.setAttribute('data-result',
                    initiallyEmpty && assigned && changes === 2 ? 'pass' :
                    'fail:' + [initiallyEmpty, assigned, changes]));
            });
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn nested_declarative_slots_and_closed_roots_keep_slotchange_in_their_tree() {
    let actual = result(
        r#"<body><div id=outer><template shadowrootmode=open>
            <div id=inner><template shadowrootmode=open><slot></slot></template>
                <b id=inner-light>inner</b></div>
        </template></div><div id=closed></div><script>
        const outer = document.getElementById('outer').shadowRoot;
        const innerHost = outer.querySelector('#inner');
        const nested = innerHost.shadowRoot;
        const innerSlot = nested.querySelector('slot');
        const closedHost = document.getElementById('closed');
        const closed = closedHost.attachShadow({mode:'closed'});
        closed.innerHTML = '<slot></slot>';
        queueMicrotask(() => {
            const closedSlot = closed.querySelector('slot'), calls = [];
            innerSlot.addEventListener('slotchange', () => calls.push('nested'));
            closedSlot.addEventListener('slotchange', () => calls.push('closed'));
            outer.addEventListener('slotchange', () => calls.push('outer'));
            closedHost.addEventListener('slotchange', () => calls.push('host'));
            const added = document.createElement('i');
            innerHost.appendChild(added);
            const light = document.createElement('i');
            closedHost.appendChild(light);
            queueMicrotask(() => document.body.setAttribute('data-result',
                innerSlot.assignedNodes().includes(added) && closedSlot.assignedNodes()[0] === light &&
                closedHost.shadowRoot === null && calls.join(',') === 'nested,closed' ?
                'pass' : 'fail:' + calls.join(',')));
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn observer_mutation_resignals_a_slot_in_the_next_delivery_batch() {
    let actual = result(
        r#"<body><div id=host></div><script>
        const host = document.getElementById('host');
        const root = host.attachShadow({mode:'open'});
        root.innerHTML = '<slot></slot>';
        queueMicrotask(() => {
            const order = [], slot = root.querySelector('slot');
            let appended = false;
            new MutationObserver(() => {
                order.push('observer');
                if (!appended) {
                    appended = true;
                    host.appendChild(document.createElement('i'));
                }
            }).observe(host, {childList:true});
            slot.addEventListener('slotchange', () => order.push('slot'));
            host.appendChild(document.createElement('b'));
            queueMicrotask(() => queueMicrotask(() =>
                document.body.setAttribute('data-result', order.join(','))));
        });
        </script>"#,
    );
    assert_eq!(actual, "observer,slot,observer,slot");
}

#[test]
fn distribution_ignores_author_overrides_across_many_and_detached_roots() {
    let actual = result(
        r#"<body><script>
        const hosts = [], events = [];
        for (let i = 0; i < 24; i++) {
            const host = document.createElement('div');
            document.body.appendChild(host);
            const root = host.attachShadow({mode: i === 0 ? 'closed' : 'open'});
            root.innerHTML = '<slot></slot>';
            const slot = root.querySelector('slot');
            hosts.push({host, root, slot});
            slot.addEventListener('slotchange', () => events.push(i));
        }
        queueMicrotask(() => {
            for (const {root, slot} of hosts) {
                root.querySelectorAll = () => { throw Error('author query override'); };
                slot.assignedNodes = () => { throw Error('author assignment override'); };
                slot.dispatchEvent = () => { throw Error('author dispatch override'); };
            }
            document.body.removeChild(hosts[0].host);
            document.body.setAttribute('data-unrelated', 'changed');
            hosts[7].host.appendChild(document.createElement('b'));
            queueMicrotask(() => document.body.setAttribute('data-result',
                events.join(',') === '7' ? 'pass' : 'fail:' + events.join(',')));
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn a_move_across_roots_coalesces_restored_assignment_without_signaling_other_roots() {
    let actual = result(
        r#"<body><script>
        const hosts = [], slots = [], events = [];
        for (let i = 0; i < 3; i++) {
            const host = document.createElement('div');
            document.body.appendChild(host);
            const root = host.attachShadow({mode:'open'});
            root.innerHTML = '<slot></slot>';
            hosts.push(host);
            slots.push(root.querySelector('slot'));
        }
        const child = document.createElement('b');
        hosts[0].appendChild(child);
        queueMicrotask(() => {
            slots.forEach((slot, index) =>
                slot.addEventListener('slotchange', () => events.push(index)));
            hosts[1].appendChild(child);
            hosts[0].appendChild(child);
            queueMicrotask(() => document.body.setAttribute('data-result',
                events.join(',') === '0,1' &&
                slots[0].assignedNodes()[0] === child && !slots[1].assignedNodes().length ?
                'pass' : 'fail:' + events.join(',')));
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn inserting_and_removing_a_slot_bearing_subtree_updates_assignment() {
    let actual = result(
        r#"<body><div id=host><b slot=label>light</b></div><script>
        const host = document.getElementById('host');
        const root = host.attachShadow({mode:'open'});
        root.innerHTML = '<section id=container></section>';
        const container = root.querySelector('#container');
        const wrapper = document.createElement('div');
        wrapper.innerHTML = '<slot name=label></slot>';
        const slot = wrapper.querySelector('slot');
        let changes = 0;
        slot.addEventListener('slotchange', () => changes++);
        queueMicrotask(() => {
            container.appendChild(wrapper);
            queueMicrotask(() => {
                const inserted = slot.assignedNodes()[0] === host.firstChild && changes === 1;
                container.removeChild(wrapper);
                queueMicrotask(() => document.body.setAttribute('data-result',
                    inserted && slot.assignedNodes().length === 0 && changes === 2 ?
                    'pass' : 'fail:' + inserted + ':' + changes));
            });
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn deep_non_slot_mutations_do_not_signal_but_fallback_changes_coalesce() {
    let actual = result(
        r#"<body><div id=host></div><script>
        const root = document.getElementById('host').attachShadow({mode:'open'});
        root.innerHTML = '<slot></slot><section><div id=deep></div></section>';
        const slot = root.querySelector('slot'), deep = root.querySelector('#deep');
        let changes = 0;
        queueMicrotask(() => {
            slot.addEventListener('slotchange', () => changes++);
            deep.appendChild(document.createElement('i'));
            deep.firstChild.appendChild(document.createElement('b'));
            const fallback = document.createElement('em');
            slot.appendChild(fallback);
            slot.removeChild(fallback);
            queueMicrotask(() => document.body.setAttribute('data-result',
                changes === 1 ? 'pass' : 'fail:' + changes));
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn manual_reassignment_notifies_both_slots_and_keeps_assign_return_void() {
    let actual = result(
        r#"<body><div id=host><b id=light>light</b></div><script>
        const host = document.getElementById('host');
        const root = host.attachShadow({mode:'open', slotAssignment:'manual'});
        root.innerHTML = '<slot></slot><slot></slot>';
        const [a, b] = root.querySelectorAll('slot');
        const light = document.getElementById('light');
        a.assign(light);
        queueMicrotask(() => {
            let aChanges = 0, bChanges = 0;
            a.addEventListener('slotchange', () => aChanges++);
            b.addEventListener('slotchange', () => bChanges++);
            const returned = b.assign(light);
            b.assign(light);
            queueMicrotask(() => document.body.setAttribute('data-result',
                returned === undefined && aChanges === 1 && bChanges === 1 &&
                !a.assignedNodes().length && b.assignedNodes()[0] === light ?
                'pass' : 'fail:' + [aChanges, bChanges, returned]));
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}

#[test]
fn manual_reassignment_evicts_a_slot_in_another_shadow_root() {
    let actual = result(
        r#"<body><div id=first><b id=light>light</b></div><div id=second></div><script>
        const first = document.getElementById('first');
        const second = document.getElementById('second');
        const a = first.attachShadow({mode:'open', slotAssignment:'manual'});
        const b = second.attachShadow({mode:'closed', slotAssignment:'manual'});
        a.innerHTML = '<slot></slot>';
        b.innerHTML = '<slot></slot>';
        const aSlot = a.querySelector('slot'), bSlot = b.querySelector('slot');
        const light = document.getElementById('light');
        aSlot.assign(light);
        queueMicrotask(() => {
            let aChanges = 0, bChanges = 0;
            aSlot.addEventListener('slotchange', () => aChanges++);
            bSlot.addEventListener('slotchange', () => bChanges++);
            bSlot.assign(light);
            queueMicrotask(() => {
                const evicted = aChanges === 1 && bChanges === 0 &&
                    !aSlot.assignedNodes().length && !bSlot.assignedNodes().length;
                second.appendChild(light);
                queueMicrotask(() => document.body.setAttribute('data-result',
                    evicted && aChanges === 1 && bChanges === 1 &&
                    bSlot.assignedNodes()[0] === light && second.shadowRoot === null ?
                    'pass' : 'fail:' + [evicted, aChanges, bChanges]));
            });
        });
        </script>"#,
    );
    assert_eq!(actual, "pass");
}
