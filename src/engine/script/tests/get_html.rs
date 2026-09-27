use super::*;

fn check_script(markup: &str) {
    let (_, outcome) = execute_html(markup);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn get_html_selects_serializable_roots_before_light_children() {
    check_script(
        r#"<body><script>
        const check = (actual, expected, label) => {
            if (actual !== expected) throw Error(label + ': ' + actual + ' != ' + expected);
        };
        const host = document.createElement('div');
        host.innerHTML = '<b>light &amp; more</b>';
        const root = host.attachShadow({mode:'open', serializable:true,
            delegatesFocus:true, clonable:true, slotAssignment:'manual'});
        root.innerHTML = '<span title="A &amp; B">shadow &lt; text</span>';
        const template = '<template shadowrootmode="open" shadowrootdelegatesfocus=""' +
            ' shadowrootserializable="" shadowrootslotassignment="manual"' +
            ' shadowrootclonable=""><span title="A &amp; B">shadow &lt; text</span></template>';
        check(host.getHTML(), '<b>light &amp; more</b>', 'default');
        check(host.getHTML({serializableShadowRoots:false}), host.innerHTML, 'default option');
        check(host.getHTML({serializableShadowRoots:true}),
            template + '<b>light &amp; more</b>', 'serializable root first');
        check(host.getHTML({shadowRoots:[root]}), template + host.innerHTML, 'explicit root');
        check(root.getHTML(), '<span title="A &amp; B">shadow &lt; text</span>', 'root children');
        check(root.innerHTML, root.getHTML(), 'legacy innerHTML remains light children');
        </script>"#,
    );
}

#[test]
fn get_html_explicit_closed_and_nested_roots_are_independent() {
    check_script(
        r#"<body><script>
        const host = document.createElement('div');
        const outer = host.attachShadow({mode:'closed', serializable:true});
        const nestedHost = document.createElement('span');
        const nested = nestedHost.attachShadow({mode:'open'});
        nested.innerHTML = '<i>deep</i>';
        nestedHost.textContent = 'light';
        outer.append(nestedHost);
        const outerTemplate = '<template shadowrootmode="closed" shadowrootserializable="">';
        if (host.shadowRoot !== null || host.getHTML() !== '') throw Error('closed default');
        if (host.getHTML({shadowRoots:[nested]}) !== '')
            throw Error('nested root serialized without outer root');
        if (host.getHTML({serializableShadowRoots:true}) !==
            outerTemplate + '<span>light</span></template>')
            throw Error('serializable closed root');
        const both = outerTemplate + '<span><template shadowrootmode="open"><i>deep</i></template>' +
            'light</span></template>';
        if (host.getHTML({serializableShadowRoots:true, shadowRoots:[nested]}) !== both ||
            host.getHTML({shadowRoots:[outer, nested]}) !== both)
            throw Error('nested explicit root');
        if (outer.getHTML({shadowRoots:new Set([nested])}) !==
            '<span><template shadowrootmode="open"><i>deep</i></template>light</span>')
            throw Error('ShadowRoot.getHTML sequence');
        </script>"#,
    );
}

#[test]
fn parsed_declarative_root_serializes_with_escaped_contents() {
    check_script(
        r#"<body><div id=host><template shadowrootmode=open shadowrootserializable
            shadowrootclonable><span title="A &amp; B">one &lt; two</span></template>
            <em>light</em></div><script>
        const host = document.getElementById('host');
        if (!host.shadowRoot || host.firstElementChild.localName !== 'em')
            throw Error('parser did not consume template');
        const html = host.getHTML({serializableShadowRoots:true});
        if (!html.startsWith('<template shadowrootmode="open" shadowrootserializable=""' +
            ' shadowrootclonable=""><span title="A &amp; B">one &lt; two</span></template>') ||
            !html.includes('<em>light</em>')) throw Error('declarative serialization: ' + html);
        if (host.innerHTML.includes('shadowrootmode')) throw Error('innerHTML leaked root');
        </script>"#,
    );
}

#[test]
fn get_html_rejects_bad_sequences_and_incompatible_receivers() {
    check_script(
        r#"<body><script>
        const host = document.createElement('div');
        const root = host.attachShadow({mode:'open'});
        const typeError = (run, label) => {
            let caught; try { run(); } catch (error) { caught = error; }
            if (!(caught instanceof TypeError)) throw Error(label);
        };
        typeError(() => host.getHTML({shadowRoots:null}), 'null sequence');
        typeError(() => host.getHTML({shadowRoots:4}), 'non-iterable sequence');
        typeError(() => host.getHTML({shadowRoots:[host]}), 'non-root item');
        typeError(() => host.getHTML(1), 'primitive Element options dictionary');
        typeError(() => root.getHTML('options'), 'primitive ShadowRoot options dictionary');
        typeError(() => Element.prototype.getHTML.call(root), 'Element receiver');
        typeError(() => ShadowRoot.prototype.getHTML.call(host), 'ShadowRoot receiver');
        typeError(() => Element.prototype.getHTML.call({}), 'plain receiver');
        if (host.getHTML(null) !== '' || host.getHTML({shadowRoots:[root, root]}) !==
            '<template shadowrootmode="open"></template>')
            throw Error('dictionary default or duplicate sequence');
        for (const descriptor of [Object.getOwnPropertyDescriptor(Element.prototype, 'getHTML'),
            Object.getOwnPropertyDescriptor(ShadowRoot.prototype, 'getHTML')]) {
            if (!descriptor.writable || !descriptor.enumerable || !descriptor.configurable ||
                descriptor.value.length !== 0)
                throw Error('getHTML Web IDL operation descriptor');
        }
        </script>"#,
    );
}

#[test]
fn get_html_uses_native_tree_not_author_overridden_accessors() {
    check_script(
        r#"<body><script>
        const host = document.createElement('div');
        const root = host.attachShadow({mode:'open', serializable:true});
        root.append(document.createElement('b'));
        host.append(document.createElement('i'));
        Object.defineProperty(host, 'shadowRoot', {get() { throw Error('forged root'); }});
        Object.defineProperty(host, 'childNodes', {get() { throw Error('forged children'); }});
        Object.defineProperty(root, 'childNodes', {get() { throw Error('forged children'); }});
        if (host.getHTML({serializableShadowRoots:true}) !==
            '<template shadowrootmode="open" shadowrootserializable=""><b></b></template><i></i>')
            throw Error('serialization read author accessors');
        const br = document.createElement('br');
        br.append(document.createTextNode('unserialized'));
        if (br.getHTML() !== '') throw Error('void element fragment must be empty');
        </script>"#,
    );
}

#[test]
fn get_html_registry_marker_compares_document_and_shadow_registry_kinds() {
    check_script(
        r#"<body><script>
        const expected = marker => '<template shadowrootmode="open"' +
            ' shadowrootserializable=""' + (marker ? ' shadowrootcustomelementregistry=""' : '') +
            '></template>';
        const inert = document.implementation.createHTMLDocument('inert');
        const host = inert.createElement('div');
        inert.body.append(host);
        const root = host.attachShadow({mode:'open', serializable:true,
            customElementRegistry:null});
        const markup = () => host.getHTML({serializableShadowRoots:true});
        if (markup() !== expected(false)) throw Error('null document + null shadow: ' + markup());
        const scoped = new CustomElementRegistry();
        scoped.initialize(inert);
        if (markup() !== expected(true)) throw Error('scoped document + null shadow: ' + markup());
        scoped.initialize(root);
        if (markup() !== expected(true)) throw Error('scoped document + scoped shadow: ' + markup());
        const active = document.createElement('div');
        active.attachShadow({mode:'open', serializable:true, customElementRegistry:scoped});
        if (active.getHTML({serializableShadowRoots:true}) !== expected(true))
            throw Error('global document + scoped shadow');
        </script>"#,
    );
}

#[test]
fn get_html_follows_registry_kind_through_cross_document_adoption() {
    check_script(
        r#"<body><script>
        const host = document.createElement('div');
        const root = host.attachShadow({mode:'open', serializable:true});
        const markup = () => host.getHTML({serializableShadowRoots:true});
        const plain = '<template shadowrootmode="open" shadowrootserializable=""></template>';
        const scoped = '<template shadowrootmode="open" shadowrootserializable=""' +
            ' shadowrootcustomelementregistry=""></template>';
        if (markup() !== plain || root.customElementRegistry !== customElements)
            throw Error('initial global root');
        const inert = document.implementation.createHTMLDocument('inert');
        inert.body.append(host);
        if (markup() !== plain || root.customElementRegistry !== null)
            throw Error('adoption into null-registry document: ' + markup());
        const local = new CustomElementRegistry();
        local.initialize(inert);
        if (markup() !== scoped) throw Error('document initialization: ' + markup());
        document.body.append(host);
        if (markup() !== plain || root.customElementRegistry !== customElements)
            throw Error('adoption into global document: ' + markup());
        </script>"#,
    );
}

#[test]
fn serializable_declarative_output_reparses_as_a_shadow_tree() {
    use super::super::binding_helpers::serialize_children_with_shadow_roots;

    let original = dom::parse_with_scripting(
        "<div><template shadowrootmode=open shadowrootserializable><span>A &amp; B</span></template><i>light</i></div>",
        true,
    );
    let host = original.elements_named("div").next().unwrap();
    let fragment =
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (true, false));
    assert_eq!(
        fragment,
        "<template shadowrootmode=\"open\" shadowrootserializable=\"\"><span>A &amp; B</span></template><i>light</i>"
    );
    let reparsed = dom::parse_with_scripting(&format!("<div>{fragment}</div>"), true);
    let round_trip_host = reparsed.elements_named("div").next().unwrap();
    let root = round_trip_host.shadow_root().expect("round-trip root");
    assert_eq!(root.text_content(), "A & B");
    assert_eq!(round_trip_host.text_content(), "light");
}

#[test]
fn serializable_null_registry_root_preserves_parser_opt_in_on_round_trip() {
    use super::super::binding_helpers::serialize_children_with_shadow_roots;

    let original = dom::parse_with_scripting(
        "<div><template shadowrootmode=open shadowrootserializable shadowrootcustomelementregistry><x-child></x-child></template></div>",
        true,
    );
    let host = original.elements_named("div").next().unwrap();
    let fragment =
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (true, false));
    assert_eq!(
        fragment,
        "<template shadowrootmode=\"open\" shadowrootserializable=\"\" shadowrootcustomelementregistry=\"\"><x-child></x-child></template>"
    );
    let null_document_markup =
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (false, true));
    assert_eq!(
        null_document_markup,
        "<template shadowrootmode=\"open\" shadowrootserializable=\"\"><x-child></x-child></template>"
    );
    assert_eq!(
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (false, false)),
        fragment,
        "a scoped owner document differs from a null shadow registry"
    );
    if let NodeData::ShadowRoot(data) = &host.shadow_root().unwrap().data {
        // Initializing a scoped registry on a null-registry root changes the
        // HTML serializer result even if its owner document is inert.
        data.registry_is_null.set(false);
    }
    assert_eq!(
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (false, true)),
        fragment
    );
    assert_eq!(
        serialize_children_with_shadow_roots(&host, true, &HashSet::new(), &|_| (false, false)),
        fragment,
        "scoped document and scoped shadow registries still need the parser marker"
    );
    let reparsed = dom::parse_with_scripting(&format!("<div>{fragment}</div>"), true);
    let root = reparsed
        .elements_named("div")
        .next()
        .unwrap()
        .shadow_root()
        .unwrap();
    assert!(matches!(&root.data, NodeData::ShadowRoot(data)
        if data.keep_registry_null.get() && !data.registry_is_global.get()));
}
