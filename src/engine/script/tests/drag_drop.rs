use super::*;

#[test]
fn synthetic_clipboard_events_keep_data_isolated_from_other_events() {
    let (dom, outcome) = execute_html(
        r#"<div></div><output>no</output><script>
            const first = new ClipboardEvent('paste', {bubbles:true, cancelable:true});
            const second = new ClipboardEvent('paste');
            first.clipboardData.setData('text/plain', 'private');
            const supplied = new DataTransfer();
            supplied.setData('text/plain', 'provided');
            const third = new ClipboardEvent('copy', {clipboardData:supplied});
            let invalid = false;
            try { new ClipboardEvent('paste', {clipboardData:{}}); }
            catch (error) { invalid = error instanceof TypeError; }
            const seen = [];
            document.querySelector('div').addEventListener('paste', event => {
                seen.push(event instanceof ClipboardEvent, event.clipboardData === first.clipboardData,
                    event.clipboardData.getData('text/plain'));
            });
            document.querySelector('div').dispatchEvent(first);
            const checks = [first instanceof Event, first.bubbles, first.cancelable,
                second.clipboardData instanceof DataTransfer,
                second.clipboardData.getData('text/plain') === '',
                third.clipboardData === supplied,
                third.clipboardData.getData('text/plain') === 'provided', invalid,
                seen.join('|') === 'true|true|private'];
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
fn data_transfer_normalizes_types_and_enforces_item_list_rules() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
            const transfer = new DataTransfer();
            transfer.setData('TEXT', 'hello');
            transfer.setData('URL', '#comment\nhttps://example.com/\n');
            let duplicate = '';
            try { transfer.items.add('again', 'text/plain'); }
            catch (error) { duplicate = error.name; }
            const file = new File(['content'], 'notes.txt', {type:'text/plain'});
            const item = transfer.items.add(file);
            const checks = [transfer instanceof DataTransfer,
                transfer.items instanceof DataTransferItemList,
                transfer.items[0] instanceof DataTransferItem,
                transfer.items[0] === transfer.items.item(0),
                transfer.getData('text') === 'hello',
                transfer.getData('url') === 'https://example.com/',
                transfer.types.join(',') === 'text/plain,text/uri-list,Files',
                duplicate === 'NotSupportedError', item.kind === 'file',
                item.getAsFile() === file, transfer.files.item(0) === file,
                transfer.files.length === 1];
            transfer.clearData('text');
            checks.push(transfer.getData('text') === '', transfer.items.length === 2);
            transfer.items.remove(0);
            checks.push(transfer.items.length === 1, transfer.files.length === 1);
            transfer.items.clear();
            checks.push(transfer.items.length === 0, transfer.files.length === 0);
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
fn replacing_drag_data_preserves_type_order_and_item_identity() {
    let (dom, outcome) = execute_html(
        r#"<output>no</output><script>
            const transfer = new DataTransfer();
            transfer.setData('text/plain', 'old');
            transfer.setData('text/uri-list', 'https://example.com/');
            const item = transfer.items[0];
            transfer.setData('TEXT', 'new');
            const checks = [transfer.types.join(',') === 'text/plain,text/uri-list',
                transfer.items[0] === item, transfer.getData('text') === 'new'];
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
fn drag_event_constructor_and_draggable_reflection_follow_html() {
    let (dom, outcome) = execute_html(
        r#"<output></output><img><a href="/x"></a><div draggable="false"></div><script>
            const image = document.querySelector('img'), link = document.querySelector('a');
            const div = document.querySelector('div'), transfer = new DataTransfer();
            const event = new DragEvent('drop', {dataTransfer:transfer, clientX:7, bubbles:true});
            const checks = [image.draggable, link.draggable, !div.draggable,
                event instanceof MouseEvent, event.dataTransfer === transfer,
                event.clientX === 7, event.bubbles];
            div.draggable = true;
            checks.push(div.getAttribute('draggable') === 'true', div.draggable);
            link.draggable = false;
            checks.push(!link.draggable, link.getAttribute('draggable') === 'false');
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
fn trusted_pointer_drag_moves_data_only_into_accepted_drop() {
    let dom = dom::parse_with_scripting(
        r#"<div id="source" draggable="true">drag</div><div id="target">drop</div>
           <output></output><script>
            const source = document.getElementById('source');
            const target = document.getElementById('target');
            const events = [];
            source.ondragstart = event => {
                events.push('start:' + event.isTrusted);
                event.dataTransfer.effectAllowed = 'copy';
                event.dataTransfer.setData('text/plain', 'payload');
            };
            source.ondrag = event => events.push('drag:' + event.dataTransfer.getData('text/plain'));
            source.ondragend = event => {
                events.push('end:' + event.dataTransfer.dropEffect);
                document.querySelector('output').textContent = events.join('|');
            };
            target.ondragenter = event => events.push('enter:' + event.dataTransfer.types[0]);
            target.ondragover = event => {
                events.push('over:' + event.dataTransfer.getData('text/plain'));
                event.preventDefault();
            };
            target.ondrop = event => events.push('drop:' + event.dataTransfer.getData('text/plain'));
        </script>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let source = dom.elements_named("div").next().unwrap();
    let target = dom.elements_named("div").nth(1).unwrap();
    for (phase, node, buttons, x) in [
        ("down", source.clone(), 1, 10.0),
        ("move", target.clone(), 1, 30.0),
        ("up", target, 0, 30.0),
    ] {
        let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
            phase,
            target: Some(node),
            button: 0,
            buttons,
            x,
            y: 10.0,
            activate: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "start:true|drag:|enter:text/plain|over:|drop:payload|end:copy"
    );
}

#[test]
fn trusted_drag_protects_file_and_string_payloads_until_drop() {
    let dom = dom::parse_with_scripting(
        r#"<div id=source draggable=true>drag</div><div id=target>drop</div>
           <output>no</output><script>
            const source = document.getElementById('source');
            const target = document.getElementById('target');
            const checks = [];
            let shared;
            source.ondragstart = event => {
                shared = event.dataTransfer;
                shared.effectAllowed = 'copy';
                shared.setData('text/plain', 'private');
                shared.items.add(new File(['secret'], 'private.txt', {type:'text/plain'}));
                checks.push(shared.files.length === 1, shared.getData('text/plain') === 'private');
            };
            target.ondragover = event => {
                const data = event.dataTransfer;
                checks.push(data === shared, data.items.length === 2,
                    data.types.join(',') === 'text/plain,Files',
                    data.files.length === 0, data.getData('text/plain') === '',
                    data.items[0].kind === 'string', data.items[1].getAsFile() === null);
                data.setData('text/plain', 'tamper');
                data.items.add('tamper', 'text/html');
                checks.push(data.types.join(',') === 'text/plain,Files');
                event.preventDefault();
            };
            target.ondrop = event => {
                const data = event.dataTransfer;
                checks.push(data === shared, data.getData('text/plain') === 'private',
                    data.files.length === 1, data.files.item(0).name === 'private.txt');
            };
            source.ondragend = () => {
                checks.push(shared.getData('text/plain') === '', shared.items.length === 2,
                    shared.files.length === 0);
                document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join(',');
            };
        </script>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let source = dom.elements_named("div").next().unwrap();
    let target = dom.elements_named("div").nth(1).unwrap();
    for (phase, node, buttons, x) in [
        ("down", source.clone(), 1, 10.0),
        ("move", target.clone(), 1, 30.0),
        ("up", target, 0, 30.0),
    ] {
        let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
            phase,
            target: Some(node),
            button: 0,
            buttons,
            x,
            y: 10.0,
            activate: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
