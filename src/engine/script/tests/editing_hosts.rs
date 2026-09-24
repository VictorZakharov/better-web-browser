use super::*;

#[test]
fn spellcheck_reflects_enumerated_values_and_inherits_from_parent() {
    let (dom, outcome) = execute_html(
        r#"<section id=parent spellcheck=true><p id=child><span id=leaf></span></p></section>
            <output>no</output><script>
            const parent = document.querySelector('#parent');
            const child = document.querySelector('#child');
            const leaf = document.querySelector('#leaf');
            const checks = [parent.spellcheck, child.spellcheck, leaf.spellcheck];
            child.spellcheck = false;
            checks.push(child.getAttribute('spellcheck') === 'false', !child.spellcheck,
                !leaf.spellcheck);
            child.setAttribute('spellcheck', 'invalid');
            checks.push(child.spellcheck, leaf.spellcheck);
            parent.spellcheck = false;
            checks.push(!child.spellcheck, !leaf.spellcheck);
            leaf.setAttribute('spellcheck', '');
            checks.push(leaf.spellcheck);
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
fn editing_commands_mutate_only_editable_selection_and_report_support() {
    let (dom, outcome) = execute_html(
        r#"<div id=editor contenteditable>abcd</div><p id=plain>plain</p>
            <output>no</output><script>
            const editor = document.querySelector('#editor');
            const text = editor.firstChild;
            const range = document.createRange();
            range.setStart(text, 1); range.setEnd(text, 3);
            getSelection().addRange(range);
            const events = [];
            editor.addEventListener('beforeinput', event => events.push('before:' + event.inputType));
            editor.addEventListener('input', event => events.push('input:' + event.inputType));
            const checks = [document.queryCommandSupported('insertText'),
                document.queryCommandEnabled('insertText'),
                !document.queryCommandSupported('copy'),
                !document.queryCommandState('insertText'),
                !document.queryCommandIndeterm('insertText'),
                document.queryCommandValue('insertText') === '',
                document.execCommand('insertText', false, 'XY'),
                editor.textContent === 'aXYd', getSelection().isCollapsed,
                events.join('|') === 'before:insertText|input:insertText'];
            const caret = getSelection().getRangeAt(0);
            checks.push(document.execCommand('delete'), editor.textContent === 'aXd');
            checks.push(document.execCommand('insertHTML', false, '<strong>Z</strong>'),
                editor.querySelector('strong')?.textContent === 'Z');
            getSelection().removeAllRanges();
            const plain = document.querySelector('#plain');
            const outside = document.createRange();
            outside.selectNodeContents(plain);
            getSelection().addRange(outside);
            checks.push(!document.queryCommandEnabled('insertText'),
                !document.execCommand('insertText', false, 'bad'), plain.textContent === 'plain',
                document.execCommand('selectAll'), !getSelection().isCollapsed);
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
fn editing_commands_respect_cancelable_beforeinput_and_text_control_caret() {
    let (dom, outcome) = execute_html(
        r#"<input value=alpha><div contenteditable>text</div><output>no</output><script>
            const input = document.querySelector('input');
            input.focus(); input.setSelectionRange(1, 4);
            const types = [];
            input.addEventListener('beforeinput', event => {
                types.push(event.inputType);
                if (event.data === 'blocked') event.preventDefault();
            });
            input.addEventListener('input', event => types.push(event.inputType));
            const checks = [document.queryCommandEnabled('insertText'),
                !document.execCommand('insertText', false, 'blocked'), input.value === 'alpha',
                document.execCommand('insertText', false, 'e'), input.value === 'aea',
                input.selectionStart === 2, input.selectionEnd === 2,
                document.execCommand('delete'), input.value === 'aa',
                !document.queryCommandSupported('bold'), !document.execCommand('bold'),
                types.join('|') === 'insertText|insertText|insertText|deleteContentBackward|deleteContentBackward'];
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
fn contenteditable_reflects_enumerated_state_and_inherits_editability() {
    let (dom, outcome) = execute_html(
        r#"<div id=outer contenteditable><span id=child>text</span>
            <div id=locked contenteditable=false><b id=inner>locked</b></div></div>
            <p id=plain></p><output>no</output><script>
            const outer = document.getElementById('outer');
            const child = document.getElementById('child');
            const locked = document.getElementById('locked');
            const inner = document.getElementById('inner');
            const plain = document.getElementById('plain');
            const checks = [outer.contentEditable === 'true', outer.isContentEditable,
                child.contentEditable === 'inherit', child.isContentEditable,
                !locked.isContentEditable, !inner.isContentEditable,
                !plain.isContentEditable, document.designMode === 'off'];
            let invalid = false;
            try { plain.contentEditable = 'invalid'; }
            catch (error) { invalid = error.name === 'SyntaxError'; }
            checks.push(invalid, !plain.hasAttribute('contenteditable'));
            plain.contentEditable = 'plaintext-only';
            checks.push(plain.isContentEditable, plain.contentEditable === 'plaintext-only');
            plain.contentEditable = 'inherit';
            checks.push(!plain.hasAttribute('contenteditable'), !plain.isContentEditable);
            document.designMode = 'on';
            checks.push(plain.isContentEditable, document.designMode === 'on');
            document.designMode = 'off';
            checks.push(!plain.isContentEditable);
            document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
