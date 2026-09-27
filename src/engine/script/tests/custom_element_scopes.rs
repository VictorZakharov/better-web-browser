use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

#[test]
fn scoped_and_global_definitions_keep_element_provenance() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const scoped = new CustomElementRegistry();
            class ScopedCard extends HTMLElement { constructor() { super(); this.source = 'scoped'; } }
            class GlobalCard extends HTMLElement { constructor() { super(); this.source = 'global'; } }
            scoped.define('x-scope-card', ScopedCard);
            customElements.define('x-scope-card', GlobalCard);
            const host = document.createElement('div');
            document.body.appendChild(host);
            const root = host.attachShadow({mode: 'open', customElementRegistry: scoped});
            const scopedCard = document.createElement('x-scope-card', {customElementRegistry: scoped});
            const globalCard = document.createElement('x-scope-card');
            root.append(scopedCard, globalCard);
            const valid = root.customElementRegistry === scoped &&
                document.customElementRegistry === customElements &&
                scopedCard.customElementRegistry === scoped && scopedCard instanceof ScopedCard &&
                globalCard.customElementRegistry === customElements && globalCard instanceof GlobalCard &&
                scoped.get('x-scope-card') === ScopedCard && customElements.get('x-scope-card') === GlobalCard;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn declarative_null_registry_requires_initialize_before_upgrade() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-null-card id="target"></x-null-card>
        </template></div><script>
            const root = document.getElementById('host').shadowRoot;
            const target = root.querySelector('#target');
            const scoped = new CustomElementRegistry();
            class ScopedCard extends HTMLElement { constructor() { super(); this.ready = true; } }
            customElements.define('x-null-card', class GlobalCard extends HTMLElement {});
            const before = root.customElementRegistry === null && target.customElementRegistry === null &&
                !(target instanceof customElements.get('x-null-card'));
            scoped.initialize(root);
            scoped.define('x-null-card', ScopedCard);
            const after = root.customElementRegistry === scoped && target.customElementRegistry === scoped &&
                target instanceof ScopedCard && target.ready === true;
            document.body.setAttribute('data-result', before && after ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn shallow_clone_preserves_scoped_shadow_registry_and_import_fallback() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const scoped = new CustomElementRegistry();
            class ScopedLeaf extends HTMLElement {}
            scoped.define('x-scoped-leaf', ScopedLeaf);
            const host = document.createElement('div');
            const root = host.attachShadow({mode: 'open', clonable: true,
                customElementRegistry: scoped});
            root.appendChild(document.createElement('x-scoped-leaf', {customElementRegistry: scoped}));
            const clone = host.cloneNode(false);
            const clonedRoot = clone.shadowRoot;
            const inert = new Document();
            const uninitialized = inert.createElementNS('http://www.w3.org/1999/xhtml', 'x-scoped-leaf');
            const imported = document.importNode(uninitialized, {customElementRegistry: scoped});
            const closedHost = document.createElement('div');
            closedHost.attachShadow({mode: 'closed', clonable: true, customElementRegistry: scoped});
            const closedClone = closedHost.cloneNode(false);
            let closedPreserved = closedClone.shadowRoot === null;
            try { closedClone.attachShadow({mode: 'closed'}); closedPreserved = false; }
            catch (error) { closedPreserved &&= error.name === 'NotSupportedError'; }
            const checks = {
                clonedRootRegistry: clonedRoot.customElementRegistry === scoped,
                clonedChildClass: clonedRoot.firstElementChild instanceof ScopedLeaf,
                clonedChildRegistry: clonedRoot.firstElementChild.customElementRegistry === scoped,
                inertRegistry: uninitialized.customElementRegistry === null,
                importedRegistry: imported.customElementRegistry === scoped,
                importedClass: imported instanceof ScopedLeaf,
                closedPreserved
            };
            const failures = Object.keys(checks).filter(key => !checks[key]);
            document.body.setAttribute('data-result', failures.length ? failures.join(',') : 'pass');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn parser_uses_null_declarative_registry_even_if_global_name_is_defined() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            class GlobalCard extends HTMLElement {}
            customElements.define('x-parser-scope', GlobalCard);
        </script><div id="host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-parser-scope id="target" data-note="kept"></x-parser-scope>
        </template></div><script>
            const root = document.getElementById('host').shadowRoot;
            const target = root.querySelector('#target');
            const before = root.customElementRegistry === null &&
                target.customElementRegistry === null &&
                !(target instanceof GlobalCard) && target.getAttribute('data-note') === 'kept';
            const scoped = new CustomElementRegistry();
            class ScopedCard extends HTMLElement {}
            scoped.initialize(root);
            scoped.define('x-parser-scope', ScopedCard);
            const after = target instanceof ScopedCard && target.customElementRegistry === scoped;
            document.body.setAttribute('data-result', before && after ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn initialize_only_fills_null_registries_and_supports_inert_documents() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const first = new CustomElementRegistry();
            const second = new CustomElementRegistry();
            const host = document.createElement('div');
            const root = host.attachShadow({mode: 'open', customElementRegistry: first});
            const nullChild = document.createElement('x-null-fill', {customElementRegistry: null});
            root.appendChild(nullChild);
            class Filled extends HTMLElement {}
            second.define('x-null-fill', Filled);
            second.initialize(root);
            const preserved = root.customElementRegistry === first &&
                nullChild.customElementRegistry === second && nullChild instanceof Filled;
            const inert = new Document();
            const pending = inert.createElementNS('http://www.w3.org/1999/xhtml', 'x-inert-fill');
            inert.appendChild(pending);
            const before = inert.customElementRegistry === null && pending.customElementRegistry === null;
            second.initialize(inert);
            class InertFilled extends HTMLElement {}
            second.define('x-inert-fill', InertFilled);
            const after = inert.customElementRegistry === second &&
                pending.customElementRegistry === second && pending instanceof InertFilled;
            document.body.setAttribute('data-result', preserved && before && after ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn adoption_remaps_global_and_null_roots_but_preserves_scoped_roots() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const inert = new Document();
            const globalHost = document.createElement('div');
            const globalRoot = globalHost.attachShadow({mode: 'open'});
            const globalChild = document.createElement('span');
            globalRoot.appendChild(globalChild);
            inert.adoptNode(globalHost);
            const inInert = globalHost.customElementRegistry === null &&
                globalRoot.customElementRegistry === null && globalChild.customElementRegistry === null;
            document.adoptNode(globalHost);
            const backInActive = globalHost.customElementRegistry === customElements &&
                globalRoot.customElementRegistry === customElements &&
                globalChild.customElementRegistry === customElements;

            const scoped = new CustomElementRegistry();
            const scopedHost = document.createElement('div');
            const scopedRoot = scopedHost.attachShadow({mode: 'open', customElementRegistry: scoped});
            const scopedChild = document.createElement('span', {customElementRegistry: scoped});
            scopedRoot.appendChild(scopedChild);
            inert.adoptNode(scopedHost);
            const retained = scopedHost.customElementRegistry === null &&
                scopedRoot.customElementRegistry === scoped && scopedChild.customElementRegistry === scoped;
            document.body.setAttribute('data-result', inInert && backInActive && retained ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn declarative_reuse_keeps_null_registry_even_with_new_attach_option() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <span id="before"></span>
        </template></div><script>
            const host = document.getElementById('host');
            const original = host.shadowRoot;
            const scoped = new CustomElementRegistry();
            const reused = host.attachShadow({mode: 'open', customElementRegistry: scoped});
            const preserved = reused === original && reused.customElementRegistry === null &&
                reused.childNodes.length === 0;
            class GlobalCard extends HTMLElement {}
            customElements.define('x-global-after', GlobalCard);
            const target = document.createElement('x-global-after', {customElementRegistry: null});
            reused.appendChild(target);
            const before = target.customElementRegistry === null && !(target instanceof GlobalCard);
            customElements.initialize(reused);
            const after = reused.customElementRegistry === customElements &&
                target.customElementRegistry === customElements && target instanceof GlobalCard;
            document.body.setAttribute('data-result', preserved && before && after ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn fragment_parsing_in_scoped_shadow_tree_uses_context_registry() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const scoped = new CustomElementRegistry();
            class ScopedFragment extends HTMLElement {}
            scoped.define('x-fragment-scope', ScopedFragment);
            const host = document.createElement('div');
            document.body.appendChild(host);
            const root = host.attachShadow({mode: 'open', customElementRegistry: scoped});
            root.innerHTML = '<x-fragment-scope id="direct"></x-fragment-scope>';
            const wrapper = document.createElement('div', {customElementRegistry: scoped});
            root.appendChild(wrapper);
            wrapper.innerHTML = '<x-fragment-scope id="nested"></x-fragment-scope>';
            const direct = root.querySelector('#direct');
            const nested = wrapper.querySelector('#nested');
            const valid = direct instanceof ScopedFragment && nested instanceof ScopedFragment &&
                direct.customElementRegistry === scoped && nested.customElementRegistry === scoped;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn import_fallback_applies_to_ordinary_descendants_not_shadow_descendants() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const html = 'http://www.w3.org/1999/xhtml';
            const inert = new Document();
            const host = inert.createElementNS(html, 'div');
            const light = inert.createElementNS(html, 'x-import-scope');
            host.appendChild(light);
            const originalRoot = host.attachShadow({mode: 'open', clonable: true,
                customElementRegistry: null});
            originalRoot.appendChild(inert.createElementNS(html, 'x-import-scope'));
            const scoped = new CustomElementRegistry();
            class Scoped extends HTMLElement {}
            scoped.define('x-import-scope', Scoped);
            const imported = document.importNode(host, {customElementRegistry: scoped});
            const root = imported.shadowRoot;
            const scopedFallback = imported.customElementRegistry === scoped &&
                imported.firstElementChild.customElementRegistry === scoped &&
                imported.firstElementChild instanceof Scoped;
            const isolatedShadow = root.customElementRegistry === null &&
                root.firstElementChild.customElementRegistry === null &&
                !(root.firstElementChild instanceof Scoped);

            class Global extends HTMLElement {}
            customElements.define('x-import-global', Global);
            const pending = inert.createElementNS(html, 'x-import-global');
            const defaulted = document.importNode(pending, {selfOnly: true});
            const omittedMeansDefault = defaulted.customElementRegistry === customElements &&
                defaulted instanceof Global;
            let nullRejected = false;
            try { document.importNode(pending, {customElementRegistry: null}); }
            catch (error) { nullRejected = error instanceof TypeError; }
            document.body.setAttribute('data-result', scopedFallback && isolatedShadow &&
                omittedMeansDefault && nullRejected ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn direct_shadow_root_copy_and_adoption_report_standard_errors() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const root = document.createElement('div').attachShadow({mode: 'closed', clonable: true});
            const rejects = (operation, name) => {
                try { operation(); return false; }
                catch (error) { return error.name === name; }
            };
            const valid = rejects(() => root.cloneNode(true), 'NotSupportedError') &&
                rejects(() => document.importNode(root, true), 'NotSupportedError') &&
                rejects(() => document.adoptNode(root), 'HierarchyRequestError');
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn one_constructor_can_be_defined_in_two_scoped_registries() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const first = new CustomElementRegistry();
            const second = new CustomElementRegistry();
            class Shared extends HTMLElement {}
            first.define('x-first-name', Shared);
            second.define('x-second-name', Shared);
            const left = document.createElement('x-first-name', {customElementRegistry: first});
            const right = document.createElement('x-second-name', {customElementRegistry: second});
            const valid = left instanceof Shared && right instanceof Shared &&
                left.localName === 'x-first-name' && right.localName === 'x-second-name' &&
                left.customElementRegistry === first && right.customElementRegistry === second &&
                first.getName(Shared) === 'x-first-name' && second.getName(Shared) === 'x-second-name';
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn declarative_keep_null_survives_cross_document_adoption() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="host"><template shadowrootmode="open"
            shadowrootcustomelementregistry><x-kept-null></x-kept-null></template></div><script>
            const host = document.getElementById('host');
            const root = host.shadowRoot;
            const child = root.firstElementChild;
            const inert = new Document();
            inert.adoptNode(host);
            const inInert = root.customElementRegistry === null && child.customElementRegistry === null;
            document.adoptNode(host);
            const inActive = root.customElementRegistry === null && child.customElementRegistry === null;
            const scoped = new CustomElementRegistry();
            scoped.initialize(root);
            const initialized = root.customElementRegistry === scoped &&
                child.customElementRegistry === scoped;
            document.body.setAttribute('data-result', inInert && inActive && initialized ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn unsupported_customized_builtin_options_are_not_silently_ignored() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const rejects = callback => {
                try { callback(); return false; }
                catch (error) { return error.name === 'NotSupportedError'; }
            };
            const scoped = new CustomElementRegistry();
            const valid = rejects(() => document.createElement('button', {is: undefined})) &&
                rejects(() => document.createElementNS('http://www.w3.org/1999/xhtml',
                    'button', {is: null})) &&
                rejects(() => scoped.define('x-not-built-in', class extends HTMLElement {},
                    {extends: undefined}));
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
