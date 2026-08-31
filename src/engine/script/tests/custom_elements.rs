use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

#[test]
fn registry_upgrades_parsed_elements_and_resolves_when_defined() {
    let (dom, outcome) = execute_html(
        r#"<body><x-profile id="parsed" data-state="ready"></x-profile><script>
            const order = [];
            const pending = customElements.whenDefined('x-profile');
            class ProfileCard extends HTMLElement {
                static get observedAttributes() { return ['data-state']; }
                constructor() { super(); order.push('constructor'); }
                attributeChangedCallback(name, oldValue, newValue) {
                    order.push('attribute:' + name + ':' + oldValue + ':' + newValue);
                }
                connectedCallback() { order.push('connected'); }
            }
            customElements.define('x-profile', ProfileCard);
            const parsed = document.getElementById('parsed');
            pending.then(constructor => {
                const valid = constructor === ProfileCard && customElements.get('x-profile') === ProfileCard &&
                    customElements.getName(ProfileCard) === 'x-profile' && parsed instanceof ProfileCard &&
                    JSON.stringify(order) === JSON.stringify([
                        'constructor', 'attribute:data-state:null:ready', 'connected'
                    ]);
                document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + JSON.stringify(order));
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn document_and_constructor_creation_preserve_custom_identity() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const connected = [];
            class CreatedElement extends HTMLElement {
                constructor() { super(); this.constructed = (this.constructed || 0) + 1; }
                connectedCallback() { connected.push(this); }
            }
            customElements.define('x-created', CreatedElement);
            const fromDocument = document.createElement('x-created');
            const fromNamespace = document.createElementNS('http://www.w3.org/1999/xhtml', 'x-created');
            const fromConstructor = new CreatedElement();
            const fragment = document.createDocumentFragment();
            const fromFragment = document.createElement('x-created');
            fragment.appendChild(fromFragment);
            document.body.append(fromDocument, fromNamespace, fromConstructor);
            document.body.appendChild(fragment);
            const clone = fromDocument.cloneNode();
            const valid = [fromDocument, fromNamespace, fromConstructor, fromFragment, clone].every(element =>
                element instanceof CreatedElement && element instanceof HTMLElement &&
                element.localName === 'x-created' && element.constructed === 1) &&
                connected.length === 4 && connected[3] === fromFragment && clone.isConnected === false;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn lifecycle_and_nested_attribute_reactions_run_in_mutation_order() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const order = [];
            class LifecycleElement extends HTMLElement {
                static get observedAttributes() { return ['data-mode']; }
                attributeChangedCallback(_name, oldValue, newValue, namespace) {
                    order.push('attribute:' + oldValue + ':' + newValue + ':' + namespace);
                    if (newValue === 'one') this.setAttribute('data-mode', 'two');
                }
                connectedCallback() { order.push('connected'); }
                disconnectedCallback() { order.push('disconnected'); }
                adoptedCallback(oldDocument, newDocument) {
                    order.push('adopted:' + (oldDocument === document) + ':' + (newDocument !== document));
                }
            }
            customElements.define('x-lifecycle', LifecycleElement);
            const element = document.createElement('x-lifecycle');
            element.setAttribute('data-mode', 'one');
            document.body.appendChild(element);
            document.body.removeChild(element);
            const otherDocument = new Document();
            otherDocument.appendChild(element);
            const valid = JSON.stringify(order) === JSON.stringify([
                'attribute:null:one:null', 'attribute:one:two:null', 'connected', 'disconnected',
                'adopted:true:true', 'connected'
            ]);
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + JSON.stringify(order));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn nested_attribute_reactions_run_before_the_reflecting_setter_returns() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            let observedDuringReflection = false;
            class ReflectingElement extends HTMLElement {
                static get observedAttributes() { return ['active']; }
                connectedCallback() {
                    this.reflecting = true;
                    this.setAttribute('active', '');
                    this.reflecting = false;
                }
                attributeChangedCallback() {
                    observedDuringReflection = this.reflecting;
                }
            }
            customElements.define('reflecting-element', ReflectingElement);
            document.body.appendChild(document.createElement('reflecting-element'));
            document.body.setAttribute('data-result', observedDuringReflection ? 'pass' : 'fail');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn registry_validation_and_explicit_fragment_upgrade_are_bounded() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            const fragment = document.createDocumentFragment();
            const candidate = document.createElement('x-manual');
            fragment.appendChild(candidate);
            class ManualElement extends HTMLElement {}
            customElements.define('x-manual', ManualElement);
            const stayedPending = !(candidate instanceof ManualElement);
            customElements.upgrade(fragment);
            const failures = [];
            const expect = (name, callback, expected) => {
                try { callback(); failures.push(name + ':missing'); }
                catch (error) { if (error.name !== expected) failures.push(name + ':' + error.name); }
            };
            expect('invalid-name', () => customElements.define('invalid', class extends HTMLElement {}), 'SyntaxError');
            expect('invalid-punctuation', () => customElements.define('x/bad', class extends HTMLElement {}), 'SyntaxError');
            expect('invalid-get-name', () => customElements.getName({}), 'TypeError');
            expect('duplicate-name', () => customElements.define('x-manual', class extends HTMLElement {}), 'NotSupportedError');
            expect('duplicate-constructor', () => customElements.define('x-other', ManualElement), 'NotSupportedError');
            expect('bad-callback', () => customElements.define('x-callback', class extends HTMLElement {
                get connectedCallback() { return 1; }
            }), 'TypeError');
            expect('customized-built-in', () => customElements.define('x-built-in', class extends HTMLElement {},
                { extends: 'button' }), 'NotSupportedError');
            expect('foreign-document', () => customElements.upgrade(document), 'NotSupportedError');
            const scoped = new CustomElementRegistry();
            class ScopedElement extends HTMLElement {}
            scoped.define('x-scoped', ScopedElement);
            const valid = stayedPending && candidate instanceof ManualElement &&
                scoped.get('x-scoped') === ScopedElement && failures.length === 0;
            document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + failures.join(','));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn html_insertion_and_document_write_upgrade_defined_elements() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="container"></div><script>
            const order = [];
            class InsertedElement extends HTMLElement {
                static get observedAttributes() { return ['data-value']; }
                constructor() { super(); order.push('constructor:' + this.id); }
                attributeChangedCallback(_name, _oldValue, newValue) {
                    order.push('attribute:' + this.id + ':' + newValue);
                }
                connectedCallback() { order.push('connected:' + this.id); }
            }
            customElements.define('x-inserted', InsertedElement);
            document.getElementById('container').innerHTML =
                '<x-inserted id="inner" data-value="one"></x-inserted>';
            document.write('<x-inserted id="written" data-value="two"></x-inserted>');
            queueMicrotask(() => {
                const inner = document.getElementById('inner');
                const written = document.getElementById('written');
                const valid = inner instanceof InsertedElement && written instanceof InsertedElement &&
                    order.includes('constructor:inner') && order.includes('attribute:inner:one') &&
                    order.includes('connected:inner') && order.includes('constructor:written') &&
                    order.includes('attribute:written:two') && order.includes('connected:written');
                document.body.setAttribute('data-result', valid ? 'pass' : 'fail:' + JSON.stringify(order));
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
