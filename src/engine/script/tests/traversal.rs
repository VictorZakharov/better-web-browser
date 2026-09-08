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
