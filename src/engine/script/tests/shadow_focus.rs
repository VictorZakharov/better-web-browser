use super::*;

fn check(script: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn active_element_retargets_into_each_shadow_tree_and_keeps_closed_roots_private() {
    check(
        r#"
        const outerHost = document.createElement('div');
        document.body.append(outerHost);
        const outer = outerHost.attachShadow({mode:'closed'});
        const innerHost = document.createElement('div');
        outer.append(innerHost);
        const inner = innerHost.attachShadow({mode:'open'});
        const field = document.createElement('input');
        inner.append(field);
        const unrelated = document.createElement('div');
        document.body.append(unrelated);
        const unrelatedRoot = unrelated.attachShadow({mode:'open'});
        if (document.activeElement !== document.body ||
            outer.activeElement !== null || inner.activeElement !== null)
            throw Error('initial active element');
        field.focus();
        if (document.activeElement !== outerHost ||
            outer.activeElement !== innerHost || inner.activeElement !== field ||
            unrelatedRoot.activeElement !== null || outerHost.shadowRoot !== null)
            throw Error('retargeted active element');
        if (Object.getOwnPropertyDescriptor(Document.prototype, 'activeElement').set ||
            Object.getOwnPropertyDescriptor(ShadowRoot.prototype, 'activeElement').set)
            throw Error('activeElement must be readonly');
        field.blur();
        if (document.activeElement !== document.body ||
            outer.activeElement !== null || inner.activeElement !== null)
            throw Error('active element after blur');
        "#,
    );
}

#[test]
fn delegated_focus_uses_autofocus_then_keeps_current_descendant() {
    check(
        r#"
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'open', delegatesFocus:true});
        const first = document.createElement('button');
        const preferred = document.createElement('input');
        preferred.autofocus = true;
        root.append(first, preferred);
        host.focus();
        if (root.activeElement !== preferred || document.activeElement !== host)
            throw Error('autofocus delegate');
        first.focus();
        host.focus();
        if (root.activeElement !== first) throw Error('preserve focused descendant');
        host.blur();
        if (root.activeElement !== null || document.activeElement !== document.body)
            throw Error('delegating host blur');
        preferred.removeAttribute('autofocus');
        host.focus();
        if (root.activeElement !== first) throw Error('first focusable delegate');
        "#,
    );
}

#[test]
fn focus_events_cross_shadow_boundaries_with_retargeted_targets() {
    check(
        r#"
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'open'});
        const input = document.createElement('input');
        root.append(input);
        const log = [];
        root.addEventListener('focusin', event => {
            log.push('root:' + (event.target === input) + ':' + event.composed);
        });
        document.addEventListener('focusin', event => {
            log.push('document:' + (event.target === host) + ':' + event.composed);
        });
        document.addEventListener('focusout', event => {
            log.push('out:' + (event.target === host) + ':' + event.composed);
        });
        input.focus();
        input.blur();
        if (log.join('|') !== 'root:true:true|document:true:true|out:true:true')
            throw Error('focus event path: ' + log.join('|'));
        "#,
    );
}

#[test]
fn non_focusable_and_inert_elements_do_not_steal_focus() {
    check(
        r#"
        const plain = document.createElement('div');
        const inert = document.createElement('div');
        inert.setAttribute('inert', '');
        const child = document.createElement('button');
        inert.append(child);
        const hidden = document.createElement('div');
        hidden.hidden = true;
        const hiddenButton = document.createElement('button');
        hidden.append(hiddenButton);
        const button = document.createElement('button');
        document.body.append(plain, inert, hidden, button);
        button.focus();
        plain.focus();
        child.focus();
        hiddenButton.focus();
        if (document.activeElement !== button) throw Error('non-focusable target stole focus');
        plain.setAttribute('tabindex', '-1');
        plain.focus();
        if (document.activeElement !== plain) throw Error('negative tabindex should focus by script');
        "#,
    );
}

#[test]
fn slotted_controls_in_an_inert_flat_tree_cannot_take_focus() {
    check(
        r#"
        const active = document.createElement('button');
        document.body.append(active);
        active.focus();
        for (const mode of ['open', 'closed']) {
            const host = document.createElement('div');
            document.body.append(host);
            const root = host.attachShadow({mode});
            const wrapper = document.createElement('div');
            wrapper.setAttribute('inert', '');
            const slot = document.createElement('slot');
            wrapper.append(slot);
            root.append(wrapper);
            const slotted = document.createElement('button');
            host.append(slotted);
            if (mode === 'closed' && slotted.assignedSlot !== null)
                throw Error('closed assignedSlot leaked');
            slotted.focus();
            if (document.activeElement !== active)
                throw Error('slotted child of inert shadow content stole focus');
        }
        "#,
    );
}

#[test]
fn unassigned_light_dom_and_suppressed_slot_fallback_cannot_take_focus() {
    check(
        r#"
        const active = document.createElement('button');
        document.body.append(active);
        active.focus();
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'open'});
        const unassigned = document.createElement('input');
        host.append(unassigned);
        unassigned.focus();
        if (document.activeElement !== active)
            throw Error('unassigned light DOM was focused');
        unassigned.remove();
        const slot = document.createElement('slot');
        const fallback = document.createElement('button');
        slot.append(fallback);
        root.append(slot);
        fallback.focus();
        if (document.activeElement !== host || root.activeElement !== fallback)
            throw Error('visible slot fallback could not focus');
        active.focus();
        host.append(unassigned);
        unassigned.focus();
        fallback.focus();
        if (document.activeElement !== unassigned)
            throw Error('assigned light DOM did not focus');
        if (root.activeElement !== null)
            throw Error('suppressed fallback retained focus');
        "#,
    );
}

#[test]
fn focus_events_expose_the_target_documents_window() {
    check(
        r#"
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'open'});
        const input = document.createElement('input');
        root.append(input);
        const seen = [];
        for (const type of ['focus', 'focusin', 'blur', 'focusout']) {
            input.addEventListener(type, event => seen.push(type + ':' + (event.view === window)));
        }
        input.focus();
        input.blur();
        if (seen.join('|') !== 'focus:true|focusin:true|blur:true|focusout:true')
            throw Error('FocusEvent.view: ' + seen.join('|'));
        "#,
    );
}

#[test]
fn tabindex_reflects_html_integer_parsing_without_overstating_focusability() {
    check(
        r#"
        const ordinary = document.createElement('div');
        const button = document.createElement('button');
        const hidden = document.createElement('input');
        hidden.type = 'hidden';
        document.body.append(ordinary, button, hidden);
        if (ordinary.tabIndex !== -1 || button.tabIndex !== 0 ||
            hidden.tabIndex !== 0) throw Error('default tabIndex');
        ordinary.setAttribute('tabindex', '  +12trailing');
        if (ordinary.tabIndex !== 12) throw Error('HTML integer parser');
        ordinary.focus();
        if (document.activeElement !== ordinary) throw Error('parsed tabindex focus');
        ordinary.tabIndex = -2;
        if (ordinary.getAttribute('tabindex') !== '-2' || ordinary.tabIndex !== -2)
            throw Error('IDL setter');
        hidden.tabIndex = 0;
        hidden.focus();
        if (document.activeElement !== ordinary) throw Error('hidden input stole focus');
        "#,
    );
}

#[test]
fn native_focus_on_a_delegating_host_targets_its_shadow_control() {
    let dom = dom::parse(
        "<body><div id=host><template shadowrootmode=open shadowrootdelegatesfocus><input id=field></template></div></body>",
    );
    let host = dom.elements_named("div").next().unwrap();
    let root = host.shadow_root().expect("declarative shadow root");
    let field = dom::Node::descendants(&root)
        .find(|node| node.tag_name() == Some("input"))
        .expect("shadow input");
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let outcome = runtime.dispatch_user_input(UserInputEvent::Focus {
        target: Some(host.clone()),
        focused: true,
    });
    assert!(
        outcome.outcome.errors.is_empty(),
        "{:?}",
        outcome.outcome.errors
    );
    assert!(field.is_focused());
    assert_eq!(runtime.focused_node_id(), Some(field.id()));
    assert!(host.has_focus_within());
    assert!(!host.is_focused());
}

#[test]
fn removing_the_focused_shadow_subtree_repairs_document_focus() {
    check(
        r#"
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'closed'});
        const input = document.createElement('input');
        root.append(input);
        input.focus();
        host.remove();
        if (document.activeElement !== document.body || root.activeElement !== null)
            throw Error('focus survived detached shadow subtree');
        "#,
    );
}

#[test]
fn replacing_focused_shadow_contents_repairs_focus_without_detaching_the_host() {
    check(
        r#"
        const host = document.createElement('div');
        document.body.append(host);
        const root = host.attachShadow({mode:'open'});
        const input = document.createElement('input');
        root.append(input);
        input.focus();
        root.innerHTML = '<button>replacement</button>';
        if (document.activeElement !== document.body || root.activeElement !== null)
            throw Error('innerHTML retained removed focus');
        const button = root.querySelector('button');
        button.focus();
        root.textContent = '';
        if (document.activeElement !== document.body || root.activeElement !== null)
            throw Error('textContent retained removed focus');
        "#,
    );
}
