use super::*;

#[test]
fn tree_walker_honors_masks_filters_order_and_root_boundaries() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><main id="root"><section id="skip"><span id="nested">nested</span></section><aside id="reject"><b id="pruned">pruned</b></aside><p id="last">last</p></main><output>no</output><script>
            const root = document.getElementById('root');
            const filter = { acceptNode(node) {
                if (node.id === 'skip') return NodeFilter.FILTER_SKIP;
                if (node.id === 'reject') return NodeFilter.FILTER_REJECT;
                return NodeFilter.FILTER_ACCEPT;
            }};
            const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, filter);
            const forward = [];
            for (let node; node = walker.nextNode();) forward.push(node.id);
            const backward = [];
            for (let node; node = walker.previousNode();) backward.push(node.id);
            walker.currentNode = root;
            const first = walker.firstChild();
            const next = walker.nextSibling();
            const parent = walker.parentNode();
            let invalidRoot = false;
            try { document.createTreeWalker(null); } catch (error) { invalidRoot = error instanceof TypeError; }
            const accepted =
                walker instanceof TreeWalker && walker.root === root && walker.filter === filter &&
                walker.whatToShow === NodeFilter.SHOW_ELEMENT &&
                forward.join(',') === 'nested,last' && backward.join(',') === 'nested' &&
                first.id === 'nested' && next.id === 'last' && parent === root && invalidRoot &&
                NodeFilter.SHOW_ALL === 0xFFFFFFFF && NodeFilter.prototype.FILTER_ACCEPT === 1;
            if (accepted) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn tree_walker_filter_reentrancy_throws_invalid_state() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><main><p></p></main><output>no</output><script>
            const root = document.querySelector('main');
            let walker;
            walker = document.createTreeWalker(root, NodeFilter.SHOW_ALL, {
                acceptNode() {
                    try { walker.nextNode(); }
                    catch (error) {
                        if (error instanceof DOMException && error.name === 'InvalidStateError')
                            document.querySelector('output').textContent = 'yes';
                    }
                    return NodeFilter.FILTER_ACCEPT;
                }
            });
            walker.nextNode();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn tree_walker_reentrancy_is_rejected_before_hidden_node_masking() {
    let (dom, outcome) = execute_html(
        r#"<main><p>text</p></main><output>no</output><script>
            const root = document.querySelector('main');
            let walker, blocked = false;
            walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, node => {
                if (node.localName === 'p') {
                    walker.currentNode = node;
                    try { walker.nextNode(); }
                    catch (error) { blocked = error.name === 'InvalidStateError'; }
                }
                return NodeFilter.FILTER_ACCEPT;
            });
            walker.nextNode();
            document.querySelector('output').textContent = blocked ? 'yes' : 'no';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_walks_root_descendants_and_rejected_subtrees_in_tree_order() {
    let (dom, outcome) = execute_html(
        r#"<main id=root><section id=skip><b id=nested></b></section><aside id=reject><i id=descendant></i></aside><p id=last></p></main><output>no</output><script>
            const root = document.getElementById('root');
            const filter = node => node.id === 'skip' ? NodeFilter.FILTER_SKIP :
                node.id === 'reject' ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT, filter);
            const firstPointer = iterator.referenceNode === root && iterator.pointerBeforeReferenceNode;
            const forward = [];
            for (let node; node = iterator.nextNode();) forward.push(node.id);
            const atEnd = iterator.referenceNode.id === 'last' && !iterator.pointerBeforeReferenceNode;
            const reverse = [];
            for (let node; node = iterator.previousNode();) reverse.push(node.id);
            const checks = [firstPointer, atEnd,
                forward.join(',') === 'root,nested,descendant,last',
                reverse.join(',') === 'last,descendant,nested,root',
                iterator.previousNode() === null,
                iterator.referenceNode === root, iterator.pointerBeforeReferenceNode,
                iterator.root === root, iterator.filter === filter,
                iterator.whatToShow === NodeFilter.SHOW_ELEMENT,
                iterator instanceof NodeIterator];
            iterator.detach();
            checks.push(iterator.nextNode() === root);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_filter_reentrancy_and_mutation_preserve_candidate_pointer() {
    let (dom, outcome) = execute_html(
        r#"<main><p id=first></p><p id=second></p><p id=third></p></main><output>no</output><script>
            const root = document.querySelector('main');
            const first = document.getElementById('first');
            const second = document.getElementById('second');
            const third = document.getElementById('third');
            let iterator, reentrant = false;
            iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT, node => {
                if (node === first) {
                    try { iterator.nextNode(); }
                    catch (error) { reentrant = error.name === 'InvalidStateError'; }
                    first.remove();
                    return NodeFilter.FILTER_SKIP;
                }
                return NodeFilter.FILTER_ACCEPT;
            });
            const checks = [iterator.nextNode() === root, iterator.nextNode() === second,
                reentrant, iterator.referenceNode === second,
                iterator.nextNode() === third, iterator.nextNode() === null,
                iterator.previousNode() === third];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_adjusts_reference_on_removal_and_tree_replacement() {
    let (dom, outcome) = execute_html(
        r#"<main><section id=section><b id=child></b></section><p id=after></p></main><output>no</output><script>
            const root = document.querySelector('main');
            const section = document.getElementById('section');
            const child = document.getElementById('child');
            const after = document.getElementById('after');
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            const checks = [iterator.nextNode() === root, iterator.nextNode() === section,
                iterator.nextNode() === child];
            section.remove();
            checks.push(iterator.referenceNode === root, !iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === after);
            root.innerHTML = '<i id="replacement"></i>';
            checks.push(iterator.referenceNode === root, !iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === root.firstChild);
            root.textContent = '';
            checks.push(iterator.referenceNode === root, iterator.nextNode() === null);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_tracks_removal_before_reference_and_moving_fragment_children() {
    let (dom, outcome) = execute_html(
        r#"<main><span id=first></span><span id=second></span><span id=third></span></main><output>no</output><script>
            const root = document.querySelector('main');
            const first = document.getElementById('first');
            const second = document.getElementById('second');
            const third = document.getElementById('third');
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            iterator.nextNode(); iterator.nextNode(); iterator.nextNode();
            const checks = [iterator.previousNode() === second, iterator.pointerBeforeReferenceNode];
            second.remove();
            checks.push(iterator.referenceNode === third, iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === third);
            const fragment = document.createDocumentFragment();
            fragment.appendChild(first);
            fragment.appendChild(third);
            root.appendChild(fragment);
            checks.push(iterator.referenceNode === root, iterator.nextNode() === first,
                iterator.nextNode() === third);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_mask_skips_hidden_nodes_without_pruning_their_children() {
    let (dom, outcome) = execute_html(
        r#"<main><section><b id=visible></b></section><aside><i id=also-visible></i></aside></main><output>no</output><script>
            const root = document.querySelector('main');
            const seen = [];
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT,
                node => {
                    seen.push(node.localName);
                    return node.localName === 'section' || node.localName === 'aside'
                        ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
                });
            const accepted = [];
            for (let node; node = iterator.nextNode();) accepted.push(node.localName);
            const textOnly = document.createNodeIterator(root, NodeFilter.SHOW_TEXT,
                node => { throw new Error('mask must skip element without invoking callback'); });
            const checks = [accepted.join(',') === 'main,b,i',
                seen.join(',') === 'main,section,b,aside,i',
                textOnly.nextNode() === null,
                textOnly.referenceNode === root, textOnly.pointerBeforeReferenceNode];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_filter_error_leaves_reference_unchanged_for_retry() {
    let (dom, outcome) = execute_html(
        r#"<main><p id=first></p><p id=second></p></main><output>no</output><script>
            const root = document.querySelector('main');
            let throwOnce = true;
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT, node => {
                if (node.id === 'first' && throwOnce) {
                    throwOnce = false;
                    throw new Error('filter failed');
                }
                return NodeFilter.FILTER_ACCEPT;
            });
            const checks = [iterator.nextNode() === root];
            let caught = false;
            try { iterator.nextNode(); } catch (error) { caught = error.message === 'filter failed'; }
            checks.push(caught, iterator.referenceNode === root,
                !iterator.pointerBeforeReferenceNode,
                iterator.nextNode().id === 'first',
                iterator.nextNode().id === 'second', iterator.nextNode() === null);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_keeps_its_root_when_the_root_moves_or_is_removed() {
    let (dom, outcome) = execute_html(
        r#"<main><section><b id=first></b></section><p id=after></p></main><output>no</output><script>
            const root = document.querySelector('section');
            const first = document.getElementById('first');
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            const checks = [iterator.nextNode() === root];
            root.remove();
            checks.push(iterator.root === root, iterator.nextNode() === first,
                iterator.nextNode() === null);
            document.querySelector('main').appendChild(root);
            checks.push(iterator.previousNode() === first, iterator.previousNode() === root,
                iterator.previousNode() === null, iterator.nextNode() === root);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_repairs_its_pointer_when_a_subtree_is_adopted() {
    let (dom, outcome) = execute_html(
        r#"<main><section><b id=inside></b></section><p id=after></p></main><output>no</output><script>
            const root = document.querySelector('main');
            const section = root.firstChild;
            const inside = document.getElementById('inside');
            const after = document.getElementById('after');
            const iterator = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            const checks = [iterator.nextNode() === root,
                iterator.nextNode() === section, iterator.nextNode() === inside];
            const otherDocument = document.implementation.createHTMLDocument('other');
            otherDocument.adoptNode(section);
            checks.push(section.ownerDocument === otherDocument,
                iterator.referenceNode === root, !iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === after, iterator.nextNode() === null);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn node_iterator_constructor_and_web_idl_argument_rules() {
    let (dom, outcome) = execute_html(
        r#"<main><p></p></main><output>no</output><script>
            const root = document.querySelector('main');
            let illegal = false, missing = false, invalid = false;
            try { new NodeIterator(); }
            catch (error) { illegal = error instanceof TypeError; }
            try { document.createNodeIterator(); }
            catch (error) { missing = error instanceof TypeError; }
            try { document.createNodeIterator(null); }
            catch (error) { invalid = error instanceof TypeError; }
            const iterator = document.createNodeIterator(root, -1, null);
            const checks = [illegal, missing, invalid,
                iterator.whatToShow === 0xffffffff,
                iterator.filter === null, iterator.root === root,
                iterator.referenceNode === root,
                iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === root,
                !iterator.pointerBeforeReferenceNode,
                iterator.nextNode() === root.firstChild];
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
