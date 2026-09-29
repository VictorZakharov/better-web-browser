use super::*;

fn check(script: &str) {
    let html = format!(
        "<body><script>function assert(value, message){{if(!value)throw Error(message)}}{script}</script>"
    );
    let (_, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn popover_attribute_reflects_state_and_selector_tracks_showing() {
    check(
        r#"
        const element = document.createElement('div');
        document.body.append(element);
        assert(element.popover === null, 'absent');
        element.popover = 'AUTO';
        assert(element.popover === 'auto', 'auto token');
        assert(!element.matches(':popover-open'), 'hidden state');
        element.showPopover();
        assert(element.matches(':popover-open'), 'shown state');
        assert(element.getAttribute('popover') === 'AUTO', 'show does not rewrite attribute');
        element.hidePopover();
        assert(!element.matches(':popover-open'), 'hidden again');
        element.popover = 'other';
        assert(element.popover === 'manual', 'invalid token is manual');
        element.popover = null;
        assert(element.popover === null, 'null removes attribute');
        let error = '';
        try { element.showPopover(); } catch (e) { error = e.name; }
        assert(error === 'NotSupportedError', 'missing attribute rejects show');
    "#,
    );
}

#[test]
fn beforetoggle_is_cancelable_on_show_and_hide_is_not() {
    check(
        r#"
        const element = document.createElement('div');
        element.popover = 'manual'; document.body.append(element);
        const events = [];
        let cancelNext = true;
        element.addEventListener('beforetoggle', event => {
            events.push(event.oldState + '>' + event.newState + ':' + event.cancelable);
            if (cancelNext) { cancelNext = false; event.preventDefault(); }
        });
        element.showPopover();
        assert(!element.matches(':popover-open'), 'canceled show');
        element.showPopover();
        assert(element.matches(':popover-open'), 'show succeeds');
        element.hidePopover();
        assert(!element.matches(':popover-open'), 'hide is not cancelable');
        assert(events.join('|') === 'closed>open:true|closed>open:true|open>closed:false', 'event sequence: ' + events);
    "#,
    );
}

#[test]
fn auto_popovers_close_peers_and_attribute_removal_closes_open_state() {
    check(
        r#"
        const first = document.createElement('div');
        const second = document.createElement('div');
        first.popover = second.popover = 'auto';
        document.body.append(first, second);
        first.showPopover(); second.showPopover();
        assert(!first.matches(':popover-open') && second.matches(':popover-open'), 'auto peer closure');
        second.removeAttribute('popover');
        assert(!second.matches(':popover-open'), 'attribute removal closes');
        first.popover = 'manual'; first.showPopover();
        second.popover = 'auto'; second.showPopover();
        assert(first.matches(':popover-open') && second.matches(':popover-open'), 'manual stays open');
        second.remove();
        assert(!second.matches(':popover-open'), 'disconnect closes');
    "#,
    );
}

#[test]
fn peer_hide_handler_cannot_leave_a_removed_requested_popover_open() {
    check(
        r#"
        const peer = document.createElement('div');
        const requested = document.createElement('div');
        peer.popover = requested.popover = 'auto';
        document.body.append(peer, requested);
        peer.showPopover();
        peer.addEventListener('beforetoggle', event => {
            if (event.newState === 'closed') requested.remove();
        });
        let failure = '';
        try { requested.showPopover(); } catch (error) { failure = error.name; }
        assert(failure === 'InvalidStateError', 'revalidate after peer handlers: ' + failure);
        assert(!requested.isConnected && !requested.matches(':popover-open'), 'removed target stays closed');
        assert(!peer.matches(':popover-open'), 'peer completed hiding');
    "#,
    );
}

#[test]
fn invoker_default_action_honors_cancellation_and_action() {
    check(
        r#"
        const trigger = document.createElement('button');
        trigger.type = 'button'; trigger.setAttribute('popovertarget', 'menu');
        const menu = document.createElement('div'); menu.id = 'menu'; menu.popover = 'auto';
        document.body.append(trigger, menu);
        const sources = [];
        menu.addEventListener('beforetoggle', event => sources.push(event.source));
        assert(trigger.popoverTargetElement === menu, 'target reflection');
        trigger.addEventListener('click', event => event.preventDefault(), { once: true });
        trigger.click();
        assert(!menu.matches(':popover-open'), 'canceled click');
        trigger.click();
        assert(menu.matches(':popover-open'), 'click opens');
        trigger.popoverTargetAction = 'hide';
        trigger.click();
        assert(!menu.matches(':popover-open'), 'hide action');
        assert(sources.length === 2 && sources.every(source => source === trigger), 'invoker source');
        const idless = document.createElement('div'); idless.popover = 'manual';
        document.body.append(idless);
        trigger.popoverTargetElement = idless;
        assert(trigger.getAttribute('popovertarget') === '' &&
            trigger.popoverTargetElement === idless, 'explicit idless target');
        trigger.popoverTargetAction = 'show'; trigger.click();
        assert(idless.matches(':popover-open'), 'idless target activation');
        trigger.setAttribute('popovertarget', 'menu');
        assert(trigger.popoverTargetElement === menu, 'content attribute resets explicit target');
    "#,
    );
}

#[test]
fn click_inside_popover_nested_in_its_invoker_does_not_toggle_it() {
    check(
        r#"
        const invoker = document.createElement('button');
        invoker.type = 'button'; invoker.setAttribute('popovertarget', 'nested');
        const popover = document.createElement('span');
        popover.id = 'nested'; popover.popover = 'manual';
        const child = document.createElement('span'); popover.append(child);
        invoker.append(popover); document.body.append(invoker);
        popover.showPopover();
        child.click();
        assert(popover.matches(':popover-open'), 'internal click must not toggle ancestor invoker');
        invoker.click();
        assert(!popover.matches(':popover-open'), 'direct invoker click still toggles');
    "#,
    );
}

#[test]
fn autofocus_and_focus_restoration_follow_popover_lifetime() {
    check(
        r#"
        const previous = document.createElement('button');
        const menu = document.createElement('div');
        const inside = document.createElement('button');
        inside.setAttribute('autofocus', '');
        menu.popover = 'auto'; menu.append(inside);
        document.body.append(previous, menu);
        previous.focus();
        menu.showPopover();
        assert(document.activeElement === inside, 'autofocus descendant');
        menu.hidePopover();
        assert(document.activeElement === previous, 'previous focus restored');
        menu.showPopover();
        previous.focus();
        menu.hidePopover();
        assert(document.activeElement === previous, 'do not steal focus moved outside');
    "#,
    );
}

#[test]
fn only_the_first_dismissible_popover_restores_focus() {
    check(
        r#"
        const origin = document.createElement('button');
        const outer = document.createElement('div'); outer.popover = 'auto';
        const invoker = document.createElement('button');
        const inner = document.createElement('div'); inner.popover = 'auto';
        const inside = document.createElement('button'); inside.setAttribute('autofocus', '');
        inner.append(inside); outer.append(invoker, inner);
        document.body.append(origin, outer);
        origin.focus(); outer.showPopover(); invoker.focus(); inner.showPopover();
        assert(document.activeElement === inside, 'inner autofocus');
        inner.hidePopover();
        assert(document.activeElement !== invoker, 'nested popover did not restore focus');
        outer.hidePopover();
        assert(document.activeElement === origin, 'stack root restored focus');

        const manual = document.createElement('div'); manual.popover = 'manual';
        const manualInside = document.createElement('button');
        manualInside.setAttribute('autofocus', ''); manual.append(manualInside);
        document.body.append(manual);
        origin.focus(); manual.showPopover(); manual.hidePopover();
        assert(document.activeElement !== origin, 'manual popover did not restore focus');
    "#,
    );
}

#[test]
fn auto_popover_opened_from_hint_joins_hint_stack() {
    check(
        r#"
        const auto = document.createElement('div'); auto.popover = 'auto';
        const hint = document.createElement('div'); hint.popover = 'hint';
        const trigger = document.createElement('button'); hint.append(trigger);
        const child = document.createElement('div'); child.popover = 'auto';
        const nextHint = document.createElement('div'); nextHint.popover = 'hint';
        document.body.append(auto, hint, child, nextHint);
        auto.showPopover(); hint.showPopover(); child.showPopover({ source: trigger });
        assert(auto.matches(':popover-open'), 'unrelated auto stays open');
        assert(hint.matches(':popover-open') && child.matches(':popover-open'), 'hint family stays open');
        nextHint.showPopover();
        assert(auto.matches(':popover-open'), 'auto survives a new hint');
        assert(!hint.matches(':popover-open') && !child.matches(':popover-open'),
            'new hint closes the previous hint stack');
    "#,
    );
}

#[test]
fn moving_an_invoker_does_not_detach_its_open_popover_from_the_stack() {
    check(
        r#"
        const outer = document.createElement('div'); outer.popover = 'auto';
        const trigger = document.createElement('button'); outer.append(trigger);
        const inner = document.createElement('div'); inner.popover = 'auto';
        document.body.append(outer, inner);
        outer.showPopover(); inner.showPopover({ source: trigger });
        assert(outer.matches(':popover-open') && inner.matches(':popover-open'), 'nested stack');
        document.body.append(trigger);
        outer.hidePopover();
        assert(!inner.matches(':popover-open'), 'moving trigger does not orphan child');
    "#,
    );
}

#[test]
fn form_owned_reset_button_can_invoke_a_popover() {
    check(
        r#"
        const form = document.createElement('form');
        const reset = document.createElement('button'); reset.type = 'reset';
        reset.setAttribute('popovertarget', 'menu'); form.append(reset);
        const menu = document.createElement('div'); menu.id = 'menu'; menu.popover = 'manual';
        document.body.append(form, menu);
        assert(reset.form === form, 'reset has form owner');
        reset.click();
        assert(menu.matches(':popover-open'), 'form-owned reset invokes popover');
    "#,
    );
}

#[test]
fn explicit_target_does_not_cross_sibling_shadow_roots() {
    check(
        r#"
        const leftHost = document.createElement('div');
        const rightHost = document.createElement('div');
        document.body.append(leftHost, rightHost);
        const left = leftHost.attachShadow({ mode: 'open' });
        const right = rightHost.attachShadow({ mode: 'open' });
        const invoker = document.createElement('button'); invoker.type = 'button';
        const unrelated = document.createElement('div'); unrelated.popover = 'manual';
        left.append(invoker); right.append(unrelated);
        invoker.popoverTargetElement = unrelated;
        assert(invoker.popoverTargetElement === null, 'sibling root getter is null');
        invoker.click();
        assert(!unrelated.matches(':popover-open'), 'sibling root does not activate');

        const local = document.createElement('div'); local.popover = 'manual';
        left.append(local); invoker.popoverTargetElement = local;
        assert(invoker.popoverTargetElement === local, 'same-root target resolves');
        invoker.click();
        assert(local.matches(':popover-open'), 'same-root target activates');
    "#,
    );
}

#[test]
fn toggle_task_coalesces_synchronous_show_and_hide() {
    let (dom, outcome) = execute_html(
        r#"<body><div popover id=menu></div><script>
            const menu = document.getElementById('menu');
            menu.addEventListener('toggle', event => document.body.setAttribute('data-toggle',
                [event.oldState, event.newState, event.cancelable].join(':')));
            menu.showPopover(); menu.hidePopover();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-toggle")
            .as_deref(),
        Some("closed:closed:false")
    );
}

#[test]
fn trusted_pointer_light_dismiss_closes_auto_but_not_manual() {
    let dom = dom::parse_with_scripting(
        r#"<body><button id=previous>Previous</button><span id=outside>Outside</span>
            <div id=auto popover><button autofocus id=inside>Inside</button></div>
            <div id=manual popover=manual>Manual</div>
            <script>document.getElementById('previous').focus();
                    document.getElementById('auto').showPopover();
                    document.getElementById('manual').showPopover();</script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#popover".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let previous = dom.elements_named("button").next().unwrap();
    let inside = dom.elements_named("button").nth(1).unwrap();
    assert!(inside.is_focused());
    let outside = dom.elements_named("span").next().unwrap();
    for (phase, buttons) in [("down", 1), ("up", 0)] {
        let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
            target: Some(outside.clone()),
            phase,
            button: 0,
            buttons,
            x: 10.0,
            y: 10.0,
            activate: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    let auto = dom.elements_named("div").next().unwrap();
    let manual = dom.elements_named("div").nth(1).unwrap();
    assert!(!auto.is_popover_open());
    assert!(manual.is_popover_open());
    assert!(
        !previous.is_focused(),
        "light dismiss must not restore prior focus"
    );
}
