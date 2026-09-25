use super::*;

fn check(script: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn outer_html_uses_parent_table_context_and_replaces_only_the_target() {
    check(
        r#"
        const table = document.createElement('table');
        table.innerHTML = '<tbody><tr><td>old</td></tr></tbody>';
        const tbody = table.firstChild;
        const original = tbody.firstChild;
        const previous = document.createElement('tr');
        previous.innerHTML = '<td>before</td>';
        tbody.insertBefore(previous, original);
        original.outerHTML = '<tr><td>new</td></tr><tr><td>next</td></tr>';
        if (tbody.children.length !== 3 || tbody.firstChild !== previous ||
            tbody.children[1].firstChild.localName !== 'td' ||
            tbody.children[1].textContent !== 'new' ||
            tbody.children[2].textContent !== 'next' || original.parentNode !== null)
            throw Error('outerHTML lost table context or neighboring nodes');
        "#,
    );
}

#[test]
fn insert_adjacent_html_uses_context_and_preserves_existing_nodes() {
    check(
        r#"
        const table = document.createElement('table');
        table.innerHTML = '<tbody><tr><td>middle</td></tr></tbody>';
        const tbody = table.firstChild, middle = tbody.firstChild;
        middle.insertAdjacentHTML('BEFOREBEGIN', '<tr><td>first</td></tr>');
        middle.insertAdjacentHTML('afterend', '<tr><td>last</td></tr>');
        middle.insertAdjacentHTML('afterbegin', '<th>head</th>');
        middle.insertAdjacentHTML('beforeend', '<td>tail</td>');
        if (tbody.children.length !== 3 || tbody.children[1] !== middle ||
            tbody.firstChild.textContent !== 'first' ||
            tbody.lastChild.textContent !== 'last' ||
            middle.firstChild.localName !== 'th' ||
            middle.lastChild.localName !== 'td' ||
            middle.children[1].textContent !== 'middle')
            throw Error('adjacent HTML did not use insertion context');
        "#,
    );
}

#[test]
fn adjacent_html_does_not_reparse_or_recreate_existing_children() {
    check(
        r#"
        const target = document.createElement('section');
        const old = document.createElement('button');
        old.textContent = 'old'; target.appendChild(old);
        let clicks = 0;
        old.addEventListener('click', () => clicks++);
        target.insertAdjacentHTML('afterbegin', '<b>new</b>');
        if (target.lastChild !== old || target.firstChild.localName !== 'b')
            throw Error('existing child was replaced');
        old.click();
        if (clicks !== 1) throw Error('existing listener was lost');
        "#,
    );
}

#[test]
fn fragment_parser_preserves_foreign_content_context() {
    check(
        r#"
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.insertAdjacentHTML('beforeend', '<circle cx="2"/>');
        if (svg.firstChild.localName !== 'circle' ||
            svg.firstChild.namespaceURI !== 'http://www.w3.org/2000/svg')
            throw Error('SVG fragment lost namespace');
        "#,
    );
}

#[test]
fn range_contextual_fragments_use_the_start_elements_namespace_and_table_mode() {
    check(
        r#"
        const table = document.createElement('table');
        const tbody = document.createElement('tbody');
        table.appendChild(tbody);
        const range = document.createRange();
        range.selectNodeContents(tbody);
        const fragment = range.createContextualFragment('<tr><td>cell</td></tr>');
        if (fragment.firstChild.localName !== 'tr' ||
            fragment.firstChild.firstChild.localName !== 'td' ||
            fragment.firstChild.textContent !== 'cell' || tbody.childNodes.length !== 0)
            throw Error('range parsed outside tbody context or mutated the context');
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        range.selectNodeContents(svg);
        const circle = range.createContextualFragment('<circle r="3"/>').firstChild;
        if (circle.localName !== 'circle' ||
            circle.namespaceURI !== 'http://www.w3.org/2000/svg')
            throw Error('range lost SVG parsing context');
        "#,
    );
}

#[test]
fn markup_insertion_reports_invalid_positions_and_document_parent() {
    check(
        r#"
        const detached = document.createElement('div');
        const fails = (name, callback) => {
            try { callback(); } catch (error) { if (error.name === name) return; }
            throw Error('expected ' + name);
        };
        fails('SyntaxError', () => detached.insertAdjacentHTML(' beforeend', '<b>x</b>'));
        fails('NoModificationAllowedError', () =>
            detached.insertAdjacentHTML('beforebegin', '<b>x</b>'));
        fails('NoModificationAllowedError', () =>
            document.documentElement.insertAdjacentHTML('afterend', '<b>x</b>'));
        fails('NoModificationAllowedError', () =>
            document.documentElement.outerHTML = '<html></html>');
        "#,
    );
}
