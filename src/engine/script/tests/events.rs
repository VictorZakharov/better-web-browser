use super::*;

fn status_after_script(body: &str) -> String {
    let html = format!(r#"<body>{body}<output id="status"></output></body>"#);
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn dom_exception_exposes_web_idl_legacy_codes_and_constants() {
    let status = status_after_script(
        r#"<script>
            const syntax = new DOMException('bad input', 'SyntaxError');
            const modern = new DOMException('not allowed', 'NotAllowedError');
            Object.defineProperty(syntax, 'name', { value: 'WrongDocumentError' });
            const getter = Object.getOwnPropertyDescriptor(DOMException.prototype, 'name').get;
            let brandCheck = '';
            try { getter.call({}); } catch (error) { brandCheck = error.name; }
            document.getElementById('status').textContent = [
                syntax.name, syntax.message, syntax.code, syntax instanceof Error,
                Object.prototype.toString.call(syntax), syntax.hasOwnProperty('message'),
                modern.code, DOMException.SYNTAX_ERR,
                DOMException.prototype.SECURITY_ERR,
                Object.getOwnPropertyDescriptor(DOMException, 'SYNTAX_ERR').writable,
                Object.getOwnPropertyDescriptor(window, 'DOMException').enumerable,
                brandCheck,
            ].join('|');
        </script>"#,
    );
    assert_eq!(
        status,
        "WrongDocumentError|bad input|12|true|[object DOMException]|false|0|12|18|false|false|TypeError"
    );
}

#[test]
fn dispatches_capture_target_and_bubble_phases_with_current_targets() {
    let status = status_after_script(
        r#"<div id="outer"><div id="parent"><button id="target"></button></div></div>
        <script>
            const outer = document.getElementById('outer');
            const parent = document.getElementById('parent');
            const target = document.getElementById('target');
            const order = [];
            const record = name => event => order.push(
                name + ':' + event.eventPhase + ':' + event.currentTarget.id + ':' + event.target.id
            );
            outer.addEventListener('check', record('outer-capture'), true);
            parent.addEventListener('check', record('parent-capture'), { capture: true });
            target.addEventListener('check', record('target-bubble'));
            target.addEventListener('check', record('target-capture'), true);
            parent.addEventListener('check', record('parent-bubble'));
            outer.addEventListener('check', record('outer-bubble'));
            const event = new Event('check', { bubbles: true });
            target.dispatchEvent(event);
            document.getElementById('status').textContent = order.join('|') +
                ';after:' + event.eventPhase + ':' + (event.currentTarget === null);
        </script>"#,
    );
    assert_eq!(
        status,
        "outer-capture:1:outer:target|parent-capture:1:parent:target|\
         target-capture:2:target:target|target-bubble:2:target:target|\
         parent-bubble:3:parent:target|outer-bubble:3:outer:target;after:0:true"
            .replace(' ', "")
    );
}

#[test]
fn propagation_controls_distinguish_current_and_later_listeners() {
    let status = status_after_script(
        r#"<div id="outer"><button id="target"></button></div>
        <script>
            const outer = document.getElementById('outer');
            const target = document.getElementById('target');
            const stopped = [];
            outer.addEventListener('stopped', event => { stopped.push('first'); event.stopPropagation(); }, true);
            outer.addEventListener('stopped', () => stopped.push('second'), true);
            target.addEventListener('stopped', () => stopped.push('target'));
            target.dispatchEvent(new Event('stopped', { bubbles: true }));

            const immediate = [];
            target.addEventListener('immediate', event => {
                immediate.push('first');
                event.stopImmediatePropagation();
            });
            target.addEventListener('immediate', () => immediate.push('second'));
            outer.addEventListener('immediate', () => immediate.push('outer'));
            target.dispatchEvent(new Event('immediate', { bubbles: true }));
            document.getElementById('status').textContent = stopped.join(',') + '/' + immediate.join(',');
        </script>"#,
    );
    assert_eq!(status, "first,second/first");
}

#[test]
fn honors_once_passive_and_cancelation() {
    let status = status_after_script(
        r#"<button id="target"></button>
        <script>
            const target = document.getElementById('target');
            let once = 0;
            target.addEventListener('once', () => once++, { once: true });
            target.dispatchEvent(new Event('once'));
            target.dispatchEvent(new Event('once'));

            let passivePrevented;
            target.addEventListener('passive', event => {
                event.preventDefault();
                passivePrevented = event.defaultPrevented;
            }, { passive: true });
            const passiveResult = target.dispatchEvent(new Event('passive', { cancelable: true }));

            const canceled = new Event('cancel', { cancelable: true });
            target.addEventListener('cancel', event => event.preventDefault());
            const cancelResult = target.dispatchEvent(canceled);
            document.getElementById('status').textContent =
                [once, passivePrevented, passiveResult, cancelResult, canceled.defaultPrevented].join(':');
        </script>"#,
    );
    assert_eq!(status, "1:false:true:false:true");
}

#[test]
fn snapshots_additions_and_observes_removals_during_dispatch() {
    let status = status_after_script(
        r#"<button id="target"></button>
        <script>
            const target = document.getElementById('target');
            const removed = [];
            const second = () => removed.push('second');
            target.addEventListener('remove', () => {
                removed.push('first');
                target.removeEventListener('remove', second);
            });
            target.addEventListener('remove', second);
            target.dispatchEvent(new Event('remove'));

            const added = [];
            const late = () => added.push('late');
            target.addEventListener('add', () => {
                added.push('first');
                target.addEventListener('add', late);
            });
            target.dispatchEvent(new Event('add'));
            target.dispatchEvent(new Event('add'));
            document.getElementById('status').textContent = removed.join(',') + '/' + added.join(',');
        </script>"#,
    );
    assert_eq!(status, "first/first,first,late");
}

#[test]
fn idl_handlers_share_listener_order_and_reactivation_rules() {
    let status = status_after_script(
        r#"<button id="target"></button>
        <script>
            const target = document.getElementById('target');
            const order = [];
            target.addEventListener('click', () => order.push('first'));
            target.onclick = () => order.push('original');
            target.addEventListener('click', () => order.push('third'));
            target.onclick = () => { order.push('replacement'); return false; };
            const firstResult = target.dispatchEvent(new Event('click', { cancelable: true }));
            const firstOrder = order.splice(0).join(',');

            target.onclick = null;
            target.onclick = () => order.push('reactivated');
            target.dispatchEvent(new Event('click'));
            document.getElementById('status').textContent =
                firstOrder + ':' + firstResult + '/' + order.join(',');
        </script>"#,
    );
    assert_eq!(
        status,
        "first,replacement,third:false/first,third,reactivated"
    );
}

#[test]
fn init_event_resets_cancelation_and_legacy_return_value() {
    let status = status_after_script(
        r#"<script>
            const event = document.createEvent('Event');
            const initial = event.defaultPrevented;
            event.initEvent('check', true, true);
            event.returnValue = false;
            const canceled = event.defaultPrevented + ':' + event.returnValue;
            event.initEvent('other', false, false);
            document.getElementById('status').textContent =
                [initial, canceled, event.type, event.bubbles, event.cancelable, event.defaultPrevented].join(':');
        </script>"#,
    );
    assert_eq!(status, "false:true:false:other:false:false:false");
}

#[test]
fn cancelation_gates_default_actions_after_listener_dispatch() {
    let status = status_after_script(
        r#"<details id="details"><summary id="target">toggle</summary></details>
        <script>
            const details = document.getElementById('details');
            const target = document.getElementById('target');
            const cancel = event => event.preventDefault();
            target.addEventListener('click', cancel);
            target.click();
            const canceled = details.open;
            target.removeEventListener('click', cancel);
            target.click();
            document.getElementById('status').textContent = canceled + ':' + details.open;
        </script>"#,
    );
    assert_eq!(status, "false:true");
}

#[test]
fn idl_handlers_use_the_same_dispatcher_on_non_node_targets() {
    let status = status_after_script(
        r#"<script>
            const request = new XMLHttpRequest();
            let changes = 0;
            request.onreadystatechange = event => {
                if (event.currentTarget === request && event.eventPhase === Event.AT_TARGET) changes++;
            };
            request.open('GET', '/resource');
            request.send();
            document.getElementById('status').textContent =
                changes + ':' + (request instanceof EventTarget);
        </script>"#,
    );
    assert_eq!(status, "1:true");
}

#[test]
fn wheel_events_expose_mouse_state_deltas_and_unit_constants() {
    let status = status_after_script(
        r#"<script>
            const event = new WheelEvent('wheel', {
                clientX: 4, clientY: 5, deltaX: 1.5, deltaY: -2, deltaZ: 3,
                deltaMode: WheelEvent.DOM_DELTA_LINE
            });
            document.getElementById('status').textContent = [
                event instanceof MouseEvent, event.clientX, event.clientY,
                event.deltaX, event.deltaY, event.deltaZ, event.deltaMode,
                WheelEvent.prototype.DOM_DELTA_PAGE
            ].join(':');
        </script>"#,
    );
    assert_eq!(status, "true:4:5:1.5:-2:3:1:2");
}
