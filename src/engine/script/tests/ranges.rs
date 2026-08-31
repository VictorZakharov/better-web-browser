use super::*;

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
