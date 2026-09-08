use super::*;

#[test]
fn executes_script_and_mutates_the_owned_dom() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="app"></main><script>
            const message = document.createElement('p');
            message.className = 'result';
            message.textContent = 'JavaScript works';
            document.getElementById('app').appendChild(message);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    let paragraph = dom.elements_named("p").next().unwrap();
    assert_eq!(paragraph.attr("class").as_deref(), Some("result"));
    assert_eq!(paragraph.text_content(), "JavaScript works");
}

#[test]
fn child_collections_keep_identity_and_refresh_after_tree_mutations() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><span></span></main><script>
            const parent = document.getElementById('parent');
            const childNodes = parent.childNodes;
            const children = parent.children;
            const sameBefore = childNodes === parent.childNodes && children === parent.children;
            parent.setAttribute('data-state', 'ready');
            const sameAfterAttribute = childNodes === parent.childNodes && children === parent.children;
            parent.appendChild(document.createTextNode('text'));
            parent.appendChild(document.createElement('strong'));
            const sameAfterInsertion = childNodes === parent.childNodes && children === parent.children;
            document.body.setAttribute('data-result', [
                sameBefore, sameAfterAttribute, sameAfterInsertion,
                childNodes.length, children.length
            ].join(':'));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true:true:true:3:2")
    );
}

#[test]
fn element_sibling_navigation_skips_non_elements_and_tracks_tree_mutations() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="first"></i>text<!--note--><b id="second"></b></main>
        <output id="result"></output><script>
            const parent = document.getElementById('parent');
            const first = document.getElementById('first');
            const text = parent.childNodes[1];
            const comment = parent.childNodes[2];
            const second = document.getElementById('second');
            const initial = [
                first.previousElementSibling === null,
                first.nextElementSibling === second,
                text.previousElementSibling === first,
                text.nextElementSibling === second,
                comment.previousElementSibling === first,
                comment.nextElementSibling === second,
                second.previousElementSibling === first,
                second.nextElementSibling === null
            ];
            const middle = document.createElement('em');
            parent.insertBefore(middle, second);
            document.getElementById('result').textContent = initial.concat([
                first.nextElementSibling === middle,
                text.nextElementSibling === middle,
                middle.previousElementSibling === first,
                middle.nextElementSibling === second,
                second.previousElementSibling === middle
            ]).join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:true:true:true:true:true:true:true:true:true:true:true:true"
    );
}

#[test]
fn character_data_exposes_the_dom_mutation_contract() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="state"></output><script>
            const text = document.createTextNode('alpha');
            const comment = document.createComment('note');
            const inherited = text instanceof CharacterData && comment instanceof CharacterData &&
                text instanceof Text && !(comment instanceof Text);
            text.appendData('-omega');
            const middle = text.substringData(6, 5);
            text.replaceData(0, 5, 'start');
            text.insertData(5, ':');
            text.deleteData(6, 6);
            let errorName = '';
            try { text.substringData(99, 1); } catch (error) { errorName = error.name; }
            document.getElementById('state').textContent = [
                inherited, middle, text.data, text.length, errorName
            ].join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:omega:start::6:IndexSizeError"
    );
}

#[test]
fn executes_classic_scripts_with_html_like_comments() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="status">waiting</div><script>
            <!--
            document.getElementById('status').textContent = 'ready';
            -->
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "ready"
    );
}

#[test]
fn document_write_moves_every_fragment_child_without_reborrowing_its_parent() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            document.write('<p id="first">one</p><p id="second">two</p>');
        </script></body>"#,
    );

    assert!(!outcome.runtime_stopped, "{:?}", outcome.errors);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(dom.elements_named("p").count(), 2);
    assert_eq!(
        dom.elements_named("p")
            .map(|node| node.text_content())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
}

#[test]
fn replace_with_accepts_nodes_and_strings_and_preserves_argument_order() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="before"></i><b id="old"></b><i id="after"></i></main>
        <script>
            const old = document.getElementById('old');
            const replacement = document.createElement('span');
            replacement.id = 'replacement';
            old.replaceWith('left', replacement, 'right');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let parent = dom
        .elements_named("main")
        .find(|node| node.attr("id").as_deref() == Some("parent"))
        .unwrap();
    let children = parent.children.borrow();
    assert_eq!(
        children
            .iter()
            .map(|node| node.text_content())
            .collect::<String>(),
        "leftright"
    );
    assert_eq!(
        children
            .iter()
            .filter_map(|node| node.attr("id"))
            .collect::<Vec<_>>(),
        ["before", "replacement", "after"]
    );
}

#[test]
fn replace_child_preserves_order_identity_and_fragment_semantics() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="before"></i><b id="old"></b><i id="after"></i></main>
        <aside id="foreign"><em id="moved"></em></aside><script>
            const parent = document.getElementById('parent');
            const old = document.getElementById('old');
            const moved = document.getElementById('moved');
            const returned = parent.replaceChild(moved, old);
            const sameNodeIsNoop = parent.replaceChild(moved, moved) === moved;
            const fragment = document.createDocumentFragment();
            const first = document.createElement('u'); first.id = 'first';
            const second = document.createElement('u'); second.id = 'second';
            fragment.append(first, second);
            const removed = parent.replaceChild(fragment, moved);
            let errorName = '';
            try { parent.replaceChild(document.createElement('q'), old); }
            catch (error) { errorName = error.name; }
            document.body.setAttribute('data-result', [
                returned === old, sameNodeIsNoop, removed === moved,
                fragment.childNodes.length, errorName
            ].join(':'));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .and_then(|body| body.attr("data-result"))
            .as_deref(),
        Some("true:true:true:0:NotFoundError")
    );
    let parent = dom
        .elements_named("main")
        .find(|node| node.attr("id").as_deref() == Some("parent"))
        .unwrap();
    assert_eq!(
        parent
            .children
            .borrow()
            .iter()
            .filter_map(|node| node.attr("id"))
            .collect::<Vec<_>>(),
        ["before", "first", "second", "after"]
    );
}

#[test]
fn mutation_observers_receive_batched_child_list_records_with_sibling_context() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="before"></i></main><aside id="old"><b id="moved"></b></aside>
        <output id="result"></output><script>
            const parent = document.getElementById('parent');
            const old = document.getElementById('old');
            const moved = document.getElementById('moved');
            const fragment = document.createDocumentFragment();
            const first = document.createElement('span'); first.id = 'first';
            const second = document.createElement('span'); second.id = 'second';
            fragment.append(first, second);
            const seen = [];
            const describe = record => [
                record.target.id || '#fragment',
                Array.from(record.addedNodes, node => node.id).join('+') || '-',
                Array.from(record.removedNodes, node => node.id).join('+') || '-',
                record.previousSibling?.id || '-',
                record.nextSibling?.id || '-'
            ].join(':');
            const observer = new MutationObserver(records => seen.push(...records.map(describe)));
            observer.observe(parent, { childList: true });
            observer.observe(old, { childList: true });
            observer.observe(fragment, { childList: true });
            parent.appendChild(fragment);
            parent.insertBefore(moved, second);
            second.remove();
            queueMicrotask(() => document.getElementById('result').textContent = seen.join('|'));
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "#fragment:-:first+second:-:-|parent:first+second:-:before:-|old:-:moved:-:-|parent:moved:-:first:second|parent:-:second:moved:-"
    );
}

#[test]
fn replacement_operations_queue_one_atomic_target_record_and_preserve_source_records() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="target"><i id="old-a"></i><b id="old-b"></b></main>
        <aside id="source"><u id="moved"></u></aside><output id="result"></output><script>
            const target = document.getElementById('target');
            const source = document.getElementById('source');
            const moved = document.getElementById('moved');
            const replacementRecords = [];
            const sourceRecords = [];
            new MutationObserver(records => replacementRecords.push(...records))
                .observe(target, { childList: true });
            new MutationObserver(records => sourceRecords.push(...records))
                .observe(source, { childList: true });
            target.replaceChildren('before', moved, 'after');
            const replacement = document.createElement('strong');
            replacement.id = 'replacement';
            target.replaceChild(replacement, moved);
            queueMicrotask(() => {
                const first = replacementRecords[0];
                const second = replacementRecords[1];
                document.getElementById('result').textContent = [
                    replacementRecords.length,
                    first.addedNodes.length,
                    first.removedNodes.length,
                    Array.from(first.addedNodes, node => node.id || node.textContent).join(','),
                    second.addedNodes[0] === replacement,
                    second.removedNodes[0] === moved,
                    second.previousSibling.textContent,
                    second.nextSibling.textContent,
                    sourceRecords.length,
                    sourceRecords[0].removedNodes[0] === moved
                ].join(':');
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "2:3:2:before,moved,after:true:true:before:after:1:true"
    );
}

#[test]
fn mutation_observer_rejects_contradictory_old_value_options() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="result"></output><script>
            const observer = new MutationObserver(() => {});
            const names = [];
            for (const options of [
                { attributes: false, attributeOldValue: true },
                { attributes: false, attributeFilter: ['id'] },
                { characterData: false, characterDataOldValue: true }
            ]) {
                try { observer.observe(document.body, options); }
                catch (error) { names.push(error.name); }
            }
            document.getElementById('result').textContent = names.join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError:TypeError:TypeError"
    );
}

#[test]
fn template_inner_html_refreshes_live_content_collections_and_queues_one_record() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="result"></output><script>
            const template = document.createElement('template');
            const nodes = template.content.childNodes;
            const records = [];
            new MutationObserver(batch => records.push(...batch)).observe(
                template.content, { childList: true });
            template.innerHTML = '<span>one</span><b>two</b>';
            queueMicrotask(() => {
                const record = records[0];
                document.getElementById('result').textContent = [
                    nodes === template.content.childNodes,
                    nodes.length,
                    record.target === template.content,
                    record.addedNodes.length,
                    record.removedNodes.length
                ].join(':');
            });
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:2:true:2:0"
    );
}

#[test]
fn parent_node_members_live_on_the_standard_interfaces_and_replace_children() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i>old</i></main><output id="result"></output><script>
            const parent = document.getElementById('parent');
            const fragment = document.createDocumentFragment();
            fragment.append('left', document.createElement('b'));
            parent.replaceChildren('before', ...fragment.childNodes, 'after');
            const text = document.createTextNode('text');
            document.getElementById('result').textContent = [
                typeof Object.getOwnPropertyDescriptor(Element.prototype, 'children')?.get,
                typeof Object.getOwnPropertyDescriptor(Document.prototype, 'children')?.get,
                typeof Object.getOwnPropertyDescriptor(DocumentFragment.prototype, 'children')?.get,
                Object.hasOwn(Element.prototype, 'append'),
                Object.hasOwn(Document.prototype, 'replaceChildren'),
                Object.hasOwn(DocumentFragment.prototype, 'querySelector'),
                !('children' in text),
                !('append' in text),
                parent.children instanceof HTMLCollection,
                parent.children === parent.children,
                parent.children.length,
                parent.textContent
            ].join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "function:function:function:true:true:true:true:true:true:true:1:beforeleftafter"
    );
}

#[test]
fn child_node_members_live_on_the_standard_interfaces_and_preserve_order() {
    let (dom, outcome) = execute_html(
        r#"<body><main id="parent"><i id="first"></i><b id="middle"></b><u id="last"></u></main>
        <output id="result"></output><script>
            const parent = document.getElementById('parent');
            const first = document.getElementById('first');
            const middle = document.getElementById('middle');
            const last = document.getElementById('last');
            first.before('before');
            first.after('after', middle);
            last.replaceWith(middle, 'tail');
            const text = parent.firstChild;
            const type = document.doctype;
            document.getElementById('result').textContent = [
                Object.hasOwn(Element.prototype, 'before'),
                Object.hasOwn(CharacterData.prototype, 'after'),
                Object.hasOwn(DocumentType.prototype, 'remove'),
                !Object.hasOwn(Node.prototype, 'replaceWith'),
                typeof text.after,
                type === null || typeof type.remove,
                parent.textContent,
                Array.from(parent.children, child => child.id).join(',')
            ].join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true:true:true:true:function:true:beforeaftertail:first,middle"
    );
}

#[test]
fn insertion_validates_document_hierarchy_before_mutating_the_tree() {
    let (dom, outcome) = execute_html(
        r#"<body><output id="result"></output><script>
            const doc = document.implementation.createHTMLDocument('title');
            const original = [...doc.childNodes];
            const fragment = doc.createDocumentFragment();
            fragment.append(doc.createElement('a'), doc.createElement('b'));
            const attempts = [
                () => doc.replaceChildren(doc),
                () => doc.replaceChildren(doc.createTextNode('text')),
                () => doc.replaceChildren(fragment),
                () => doc.replaceChildren(doc.createElement('a'), doc.createElement('b')),
                () => doc.body.appendChild(doc.doctype)
            ];
            const names = attempts.map(attempt => {
                try { attempt(); return 'none'; }
                catch (error) { return error.name; }
            });
            document.getElementById('result').textContent = [
                doc.doctype?.name,
                names.join(','),
                original.every((node, index) => doc.childNodes[index] === node)
            ].join(':');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "html:HierarchyRequestError,HierarchyRequestError,HierarchyRequestError,HierarchyRequestError,HierarchyRequestError:true"
    );
}
