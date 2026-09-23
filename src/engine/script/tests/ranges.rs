use super::*;

#[test]
fn live_ranges_adjust_for_insert_remove_and_moved_subtrees() {
    let (dom, outcome) = execute_html(
        r#"<div id=left><b></b><i><em>x</em></i><u></u></div>
            <div id=right></div><output>no</output><script>
            const left = document.getElementById('left');
            const right = document.getElementById('right');
            const moved = left.children[1];
            const range = document.createRange();
            range.selectNodeContents(moved.firstChild);
            const sibling = document.createRange();
            sibling.selectNode(moved);
            left.insertBefore(document.createElement('span'), left.firstChild);
            const checks = [sibling.startOffset === 2, sibling.endOffset === 3];
            right.appendChild(moved);
            checks.push(range.startContainer === left, range.startOffset === 2,
                range.collapsed, sibling.startOffset === 2, sibling.endOffset === 2);
            left.removeChild(left.firstChild);
            checks.push(range.startOffset === 1, sibling.startOffset === 1);
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
fn live_ranges_track_character_replacement_split_and_html_replacement() {
    let (dom, outcome) = execute_html(
        r#"<div id=host><span>abcdef</span></div><output>no</output><script>
            const host = document.getElementById('host');
            const text = host.firstChild.firstChild;
            const range = document.createRange();
            range.setStart(text, 1); range.setEnd(text, 6);
            text.replaceData(2, 2, 'Z');
            const checks = [text.data === 'abZef', range.startOffset === 1,
                range.endOffset === 5];
            const tail = text.splitText(3);
            checks.push(tail.data === 'ef', range.startContainer === text,
                range.endContainer === tail, range.endOffset === 2);
            host.innerHTML = '<strong>new</strong>';
            checks.push(range.startContainer === host, range.endContainer === host,
                range.collapsed, range.startOffset === 0);
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
fn static_range_does_not_track_mutations_and_selection_extend_preserves_anchor() {
    let (dom, outcome) = execute_html(
        r#"<div id=host><span>a</span><span>b</span></div><output>no</output><script>
            const host = document.getElementById('host');
            const stable = new StaticRange({startContainer: host, startOffset: 1,
                endContainer: host, endOffset: 2});
            const selection = getSelection();
            selection.collapse(host, 2);
            selection.extend(host, 0);
            const checks = [stable instanceof AbstractRange, stable.startOffset === 1,
                selection.anchorOffset === 2, selection.focusOffset === 0,
                selection.toString() === 'ab'];
            host.insertBefore(document.createElement('em'), host.firstChild);
            checks.push(stable.startOffset === 1, stable.endOffset === 2,
                selection.anchorOffset === 3, selection.focusOffset === 0);
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
fn ranges_validate_boundaries_compare_and_clone_selected_contents() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><main><span id="a">alpha</span><span id="b">beta</span></main><output>no</output><script>
            const main = document.querySelector('main');
            const a = document.getElementById('a');
            const b = document.getElementById('b');
            const range = new Range();
            range.setStartBefore(a);
            range.setEndAfter(b);
            const clone = range.cloneRange();
            let badOffset = false;
            try { range.setStart(a.firstChild, 99); }
            catch (error) { badOffset = error instanceof DOMException && error.name === 'IndexSizeError'; }
            const accepted =
                range instanceof AbstractRange && document.createRange() instanceof Range &&
                clone.startContainer === main && clone.startOffset === 0 && clone.endOffset === 2 &&
                clone.commonAncestorContainer === main && !clone.collapsed &&
                clone.compareBoundaryPoints(Range.START_TO_START, range) === 0 &&
                clone.cloneContents().textContent === 'alphabeta' &&
                clone.intersectsNode(a) && clone.isPointInRange(main, 1) && badOffset &&
                Range.prototype.END_TO_START === 3;
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
fn contextual_fragments_and_selection_preserve_dom_wrapper_identity() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><main>start</main><output>no</output><script>
            const main = document.querySelector('main');
            const range = document.createRange();
            range.selectNodeContents(main);
            const fragment = range.createContextualFragment('<strong>ready</strong>');
            const selection = getSelection();
            selection.setBaseAndExtent(main.firstChild, 0, main.firstChild, 5);
            const accepted = fragment.firstChild instanceof HTMLElement &&
                fragment.firstChild.localName === 'strong' && fragment.textContent === 'ready' &&
                selection === document.getSelection() && selection instanceof Selection &&
                selection.rangeCount === 1 && selection.anchorNode === main.firstChild &&
                selection.focusOffset === 5 && selection.toString() === 'start';
            selection.removeAllRanges();
            if (accepted && selection.type === 'None') document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn range_clone_extract_and_delete_span_partially_selected_elements() {
    for operation in ["cloneContents", "extractContents", "deleteContents"] {
        let (dom, outcome) = execute_html(&format!(
            r#"<main><b>ab</b><i>cd</i><u>ef</u></main><output>no</output><script>
                const main = document.querySelector('main');
                const range = document.createRange();
                range.setStart(main.firstChild.firstChild, 1);
                range.setEnd(main.lastChild.firstChild, 1);
                const fragment = range.{operation}();
                const copied = fragment?.textContent ?? '';
                const expected = '{operation}' === 'cloneContents' ? 'abcdef' : 'af';
                const accepted = main.textContent === expected &&
                    (copied === ('{operation}' === 'deleteContents' ? '' : 'bcde')) &&
                    (fragment === undefined ||
                     Array.from(fragment.childNodes).map(node => node.localName).join(',') === 'b,i,u') &&
                    ('{operation}' === 'cloneContents' ||
                     (range.collapsed && range.startContainer === main && range.startOffset === 1));
                document.querySelector('output').textContent = accepted ? 'yes' :
                    [main.textContent, copied, range.startOffset].join('|');
            </script>"#,
        ));
        assert!(
            outcome.errors.is_empty(),
            "{operation}: {:?}",
            outcome.errors
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            "yes",
            "{operation}"
        );
    }
}

#[test]
fn nested_range_extraction_preserves_partial_ancestor_structure() {
    let (dom, outcome) = execute_html(
        r#"<main><section><b>a<i>bc</i></b><em>de</em></section><p>fg</p></main>
            <output>no</output><script>
            const main = document.querySelector('main');
            const start = main.querySelector('i').firstChild;
            const end = main.querySelector('p').firstChild;
            const range = document.createRange();
            range.setStart(start, 1);
            range.setEnd(end, 1);
            const fragment = range.extractContents();
            const accepted = fragment.textContent === 'cdef' &&
                fragment.firstChild.localName === 'section' &&
                fragment.firstChild.firstChild.localName === 'b' &&
                fragment.firstChild.firstChild.firstChild.localName === 'i' &&
                fragment.lastChild.localName === 'p' &&
                main.textContent === 'abg' && range.collapsed;
            document.querySelector('output').textContent = accepted ? 'yes' :
                [fragment.textContent, main.textContent, range.collapsed].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn text_split_and_range_insertion_update_collapsed_end_boundary() {
    let (dom, outcome) = execute_html(
        r#"<main>abcd</main><output>no</output><script>
            const main = document.querySelector('main');
            const text = main.firstChild;
            const range = document.createRange();
            range.setStart(text, 2);
            range.collapse(true);
            const inserted = document.createElement('b');
            inserted.textContent = 'X';
            range.insertNode(inserted);
            const checks = [main.textContent === 'abXcd', main.childNodes.length === 3,
                main.firstChild === text, main.childNodes[1] === inserted,
                !range.collapsed, range.startContainer === text, range.startOffset === 2,
                range.endContainer === main, range.endOffset === 2];
            let badOffset = false;
            try { text.splitText(99); } catch (error) { badOffset = error.name === 'IndexSizeError'; }
            checks.push(badOffset);
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
fn surround_contents_rejects_partially_selected_elements() {
    let (dom, outcome) = execute_html(
        r#"<main><b>ab</b><i>cd</i></main><output>no</output><script>
            const main = document.querySelector('main');
            const range = document.createRange();
            range.setStart(main.firstChild.firstChild, 1);
            range.setEnd(main.lastChild.firstChild, 1);
            let invalid = false;
            try { range.surroundContents(document.createElement('strong')); }
            catch (error) { invalid = error.name === 'InvalidStateError'; }
            const other = document.createRange();
            other.selectNode(main.firstChild);
            const wrapper = document.createElement('span');
            wrapper.appendChild(document.createTextNode('old'));
            other.surroundContents(wrapper);
            document.querySelector('output').textContent =
                invalid && main.textContent === 'abcd' && wrapper.textContent === 'ab' &&
                wrapper.firstChild.localName === 'b' && other.startContainer === main ? 'yes' : 'no';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
