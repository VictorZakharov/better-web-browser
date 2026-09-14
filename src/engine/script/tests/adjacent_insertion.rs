use super::*;

fn check(script: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn adjacent_element_positions_preserve_identity_and_fragment_parents() {
    check(
        r#"
        const parent = document.createDocumentFragment();
        const target = document.createElement('section');
        parent.append(target);
        const children = parent.childNodes;
        for (const position of ['beforebegin', 'afterbegin', 'beforeend', 'afterend']) {
            const node = document.createElement('i'); node.id = position;
            if (target.insertAdjacentElement(position.toUpperCase(), node) !== node)
                throw Error('return identity');
        }
        if (children !== parent.childNodes || children.length !== 3 ||
            parent.firstChild.id !== 'beforebegin' || parent.lastChild.id !== 'afterend' ||
            target.firstChild.id !== 'afterbegin' || target.lastChild.id !== 'beforeend')
            throw Error('insertion order or live collection');
        const orphan = document.createElement('aside');
        if (orphan.insertAdjacentElement('beforebegin', target) !== null ||
            orphan.insertAdjacentElement('afterend', target) !== null || target.parentNode !== parent)
            throw Error('orphan insertion must not detach its argument');
    "#,
    );
}

#[test]
fn adjacent_insertion_validates_arguments_and_hierarchy_before_mutation() {
    check(
        r#"
        function raises(name, operation) {
            try { operation(); } catch (error) {
                if (error.name === name) return;
                throw error;
            }
            throw Error('missing ' + name);
        }
        const target = document.createElement('main');
        const child = document.createElement('div'); target.append(child);
        for (const invalid of ['', ' beforeend', 'beforeend ', 'BEFOREEND\u212A'])
            raises('SyntaxError', () => target.insertAdjacentElement(invalid, child));
        for (const invalid of [null, {}, document.createTextNode('x'), document.createDocumentFragment()])
            raises('TypeError', () => target.insertAdjacentElement('bad position', invalid));
        raises('TypeError', () => target.insertAdjacentElement('beforeend'));
        raises('TypeError', () => target.insertAdjacentElement(Symbol(), child));
        raises('TypeError', () => Element.prototype.insertAdjacentElement.call({}, 'beforeend', child));
        raises('HierarchyRequestError', () => child.insertAdjacentElement('afterbegin', target));
        raises('HierarchyRequestError', () => target.insertAdjacentElement('afterbegin', target));
        raises('HierarchyRequestError', () => document.documentElement.insertAdjacentElement('afterend', child));
        if (target.firstChild !== child || child.parentNode !== target) throw Error('invalid insertion mutated');
    "#,
    );
}

#[test]
fn adjacent_text_inserts_literal_text_without_reparsing_existing_children() {
    check(
        r#"
        const target = document.createElement('main'); document.body.append(target);
        const child = document.createElement('i'); target.append(child);
        const observer = new MutationObserver(() => {});
        observer.observe(target, {childList:true});
        const literal = '<b>&amp;\u0000\r\n';
        if (target.insertAdjacentText('afterbegin', literal) !== undefined ||
            target.firstChild.nodeType !== 3 || target.firstChild.data !== literal ||
            target.lastChild !== child) throw Error('text was parsed or children replaced');
        target.insertAdjacentText('beforeend', 'end');
        target.insertAdjacentText('beforebegin', 'before');
        target.insertAdjacentText('afterend', 'after');
        if (target.previousSibling.data !== 'before' || target.nextSibling.data !== 'after' ||
            target.lastChild.data !== 'end') throw Error('text order');
        const records = observer.takeRecords();
        if (records.length !== 2 || records.some(r => r.addedNodes.length !== 1 || r.removedNodes.length))
            throw Error('text mutation records');
        try { target.insertAdjacentText('beforeend'); throw Error('missing TypeError'); }
        catch(e) { if (e.name !== 'TypeError') throw e; }
        try { target.insertAdjacentText('bad', 'text'); throw Error('missing SyntaxError'); }
        catch(e) { if (e.name !== 'SyntaxError') throw e; }
    "#,
    );
}

#[test]
fn adjacent_moves_run_normal_mutation_records_and_custom_element_lifecycle() {
    check(
        r#"
        const events = [];
        customElements.define('x-adjacent', class extends HTMLElement {
            connectedCallback() { events.push('connected'); }
            disconnectedCallback() { events.push('disconnected'); }
        });
        const source = document.createElement('main'), destination = document.createElement('aside');
        document.body.append(source, destination);
        const node = document.createElement('x-adjacent'); source.append(node); events.length = 0;
        const observer = new MutationObserver(() => {});
        observer.observe(source, {childList:true}); observer.observe(destination, {childList:true});
        destination.insertAdjacentElement('afterbegin', node);
        const records = observer.takeRecords();
        if (records.length !== 2 || records[0].target !== source || records[0].removedNodes[0] !== node ||
            records[1].target !== destination || records[1].addedNodes[0] !== node ||
            events.join(',') !== 'disconnected,connected') throw Error('move lifecycle');
        const foreign = document.implementation.createHTMLDocument('other');
        const foreignNode = foreign.createElement('span'); foreign.body.append(foreignNode);
        destination.insertAdjacentElement('beforeend', foreignNode);
        if (foreignNode.ownerDocument !== document || foreign.body.firstChild !== null)
            throw Error('adoption');
    "#,
    );
}
