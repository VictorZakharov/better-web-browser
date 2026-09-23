use super::*;

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
