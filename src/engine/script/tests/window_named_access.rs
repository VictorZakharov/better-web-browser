use super::*;

fn assert_named_access_checks(html: &str) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn window_named_properties_are_visible_immediately_after_append() {
    assert_named_access_checks(
        r#"<body><div id="initial"></div><output id="status">no</output><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            check('initial', window.initial === document.getElementById('initial'));
            const container = document.createElement('section');
            container.innerHTML = '<div id="inserted"></div><form name="namedForm"></form>';
            check('detached-id', window.inserted === undefined);
            check('detached-name', window.namedForm === undefined);
            document.body.appendChild(container);
            check('insert', window.inserted === container.firstChild && window.namedForm === container.lastChild);
            container.firstChild.id = 'renamed';
            check('rename-old', window.inserted === undefined);
            check('rename-new', window.renamed === container.firstChild);
            container.remove();
            check('remove-id', window.renamed === undefined);
            check('remove-name', window.namedForm === undefined);
            document.getElementById('status').textContent = failures.length ? failures.join(',') : 'yes';
        </script></body>"#,
    );
}

#[test]
fn window_named_properties_follow_duplicate_tree_order_and_identity_changes() {
    assert_named_access_checks(
        r#"<body><output id="status">no</output><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const parent = document.createElement('section');
            document.body.appendChild(parent);
            const first = document.createElement('div'); first.id = 'shared';
            const second = document.createElement('div'); second.id = 'shared';
            const third = document.createElement('div'); third.id = 'shared';
            parent.appendChild(first);
            check('first', window.shared === first);
            parent.appendChild(second); parent.appendChild(third);
            check('duplicates', window.shared.length === 3 &&
                window.shared[0] === first && window.shared[1] === second && window.shared[2] === third);
            parent.insertBefore(third, first);
            check('move', window.shared.length === 3 &&
                window.shared[0] === third && window.shared[1] === first && window.shared[2] === second);
            first.remove();
            check('remove', window.shared.length === 2 &&
                window.shared[0] === third && window.shared[1] === second);
            third.id = 'renamed';
            check('rename', window.shared === second && window.renamed === third);
            second.remove();
            check('last-remove', window.shared === undefined);
            document.getElementById('status').textContent = failures.join(',') || 'yes';
        </script></body>"#,
    );
}

#[test]
fn window_named_properties_respect_tree_scope_and_author_collisions() {
    assert_named_access_checks(
        r#"<body><output id="status">no</output><script>
            const failures = [];
            const check = (name, condition) => { if (!condition) failures.push(name); };
            const fragment = document.createDocumentFragment();
            const detached = document.createElement('span'); detached.id = 'fragmentName';
            fragment.appendChild(detached);
            check('fragment', window.fragmentName === undefined);
            const host = document.createElement('div');
            document.body.appendChild(host);
            const shadow = host.attachShadow({mode: 'open'});
            const shadowChild = document.createElement('span'); shadowChild.id = 'shadowName';
            shadow.appendChild(shadowChild);
            check('shadow', window.shadowName === undefined);
            document.body.appendChild(fragment);
            check('fragment-insert', window.fragmentName === detached);
            const ineligible = document.createElement('div'); ineligible.setAttribute('name', 'divName');
            document.body.appendChild(ineligible);
            check('ineligible-name', window.divName === undefined);
            window.authorValue = 42;
            const author = document.createElement('div'); author.id = 'authorValue';
            document.body.appendChild(author);
            check('author', window.authorValue === 42);
            const builtin = document.createElement('div'); builtin.id = 'document';
            document.body.appendChild(builtin);
            check('builtin', window.document === document);
            const named = document.createElement('div'); named.id = 'overridden';
            document.body.appendChild(named);
            check('named', window.overridden === named);
            window.overridden = 'author override';
            named.remove();
            check('override', window.overridden === 'author override');
            document.getElementById('status').textContent = failures.join(',') || 'yes';
        </script></body>"#,
    );
}

#[test]
fn window_named_properties_ignore_values_removed_by_attribute_reactions() {
    assert_named_access_checks(
        r#"<body><output id="status">no</output><script>
            class ReactiveName extends HTMLElement {
                static get observedAttributes() { return ['id']; }
                attributeChangedCallback(name, oldValue, newValue) {
                    if (newValue && window[newValue] !== this) this.sawStaleName = true;
                    if (newValue === 'transient') this.removeAttribute('id');
                    if (newValue === 'renamedAway') this.id = 'finalName';
                }
            }
            customElements.define('reactive-name', ReactiveName);
            const element = document.createElement('reactive-name');
            document.body.appendChild(element);
            element.id = 'transient';
            const removed = !('transient' in window) &&
                !Object.prototype.hasOwnProperty.call(window, 'transient');
            element.id = 'renamedAway';
            const renamed = !('renamedAway' in window) &&
                !Object.prototype.hasOwnProperty.call(window, 'renamedAway') &&
                window.finalName === element;
            element.attributes.getNamedItem('id').value = 'transient';
            const attrValue = !('transient' in window);
            const attribute = document.createAttribute('id');
            attribute.value = 'renamedAway';
            element.setAttributeNode(attribute);
            const attrNode = !('renamedAway' in window) && window.finalName === element;
            document.getElementById('status').textContent =
                removed && renamed && attrValue && attrNode && !element.sawStaleName ? 'yes' : 'no';
        </script></body>"#,
    );
}
