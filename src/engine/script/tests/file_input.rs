use super::*;

#[test]
fn filelist_assignment_updates_value_formdata_and_required_validation() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=form><input id=upload name=upload type=file required></form><p id=status></p>
        <script>
            const input = document.getElementById('upload');
            const first = input.files;
            const initiallyMissing = input.validity.valueMissing && first.length === 0 &&
                first.item(0) === null && input.files === first;
            const transfer = new DataTransfer();
            transfer.items.add(new File(['hello'], 'note.txt', {type: 'text/plain'}));
            input.files = transfer.files;
            const selected = input.files.length === 1 && input.files[0].name === 'note.txt' &&
                input.value === 'C:\\fakepath\\note.txt' && !input.validity.valueMissing &&
                input.checkValidity() && new FormData(document.getElementById('form')).get('upload') === input.files[0];
            let rejected = false;
            try { input.value = 'arbitrary.txt'; } catch (error) {
                rejected = error.name === 'InvalidStateError';
            }
            input.value = '';
            const cleared = input.files.length === 0 && input.value === '' &&
                input.validity.valueMissing;
            input.files = transfer.files;
            document.getElementById('form').reset();
            const reset = input.files.length === 0 && input.value === '' &&
                input.validity.valueMissing;
            document.getElementById('status').textContent = String(
                initiallyMissing && selected && rejected && cleared && reset);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn file_selection_clears_across_type_transitions_and_non_file_inputs_have_null_files() {
    let (dom, outcome) = execute_html(
        r#"<body><p id=status></p><script>
            const input = document.createElement('input');
            input.type = 'file';
            const transfer = new DataTransfer();
            transfer.items.add(new File(['x'], 'x.txt'));
            input.files = transfer.files;
            input.type = 'text';
            const text = input.files === null;
            input.type = 'file';
            document.getElementById('status').textContent = String(
                text && input.files.length === 0 && input.value === '');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn multiple_file_selection_keeps_order_and_formdata_uses_file_objects() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=form><input id=upload type=file name=photos multiple required></form>
        <p id=status></p><script>
            const input = document.getElementById('upload');
            const transfer = new DataTransfer();
            const first = new File(['one'], 'one.png', {type:'image/png'});
            const second = new File(['two'], 'two.png', {type:'image/png'});
            transfer.items.add(first); transfer.items.add(second);
            const live = transfer.files;
            input.files = live;
            transfer.items.remove(0);
            const entries = new FormData(document.getElementById('form')).getAll('photos');
            const selected = input.files === live && transfer.files === live &&
                input.files.length === 1 && input.files.item(0) === second &&
                input.files.item(1) === null && input.files[0] === second &&
                input.value === 'C:\\fakepath\\two.png' &&
                entries.length === 1 && entries[0] === second &&
                input.checkValidity();
            input.files = null;
            document.getElementById('status').textContent = String(selected &&
                input.files === live && input.files.length === 1 &&
                !input.validity.valueMissing);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn assigned_data_transfer_filelist_is_same_object_live_and_keeps_native_value_current() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=form><input id=upload name=upload type=file required></form>
        <p id=status></p><script>
            const input = document.getElementById('upload');
            const transfer = new DataTransfer();
            const live = transfer.files;
            input.files = live;
            const empty = input.files === live && transfer.files === live &&
                input.value === '' && input.validity.valueMissing;
            const first = new File(['a'], 'first.txt');
            const second = new File(['b'], 'second.txt');
            transfer.items.add(first);
            transfer.items.add('ignored', 'text/plain');
            transfer.items.add(second);
            const grown = input.files === live && live.length === 2 &&
                live[0] === first && live.item(1) === second &&
                Array.from(live)[0] === first && Array.from(live)[1] === second &&
                input.value === 'C:\\fakepath\\first.txt' && !input.validity.valueMissing &&
                new FormData(input.form).getAll('upload').length === 2;
            transfer.items.remove(0);
            const shifted = live.length === 1 && live[0] === second &&
                !('1' in live) && input.value === 'C:\\fakepath\\second.txt' &&
                new FormData(input.form).get('upload') === second;
            transfer.items.clear();
            const cleared = live.length === 0 && !('0' in live) &&
                input.value === '' && input.validity.valueMissing;
            transfer.items.add(first);
            for (let index = 1; index < 130; index++)
                transfer.items.add(new File(['x'], 'file-' + index + '.txt'));
            const bounded = live.length === 130 && input.files === live &&
                input.value === 'C:\\fakepath\\first.txt' && !input.validity.valueMissing &&
                new FormData(input.form).getAll('upload').length === 130;
            document.getElementById('status').textContent = String(
                empty && grown && shifted && cleared && bounded);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn picker_requires_user_activation_and_delivers_private_file_snapshot_before_events() {
    use crate::renderer_protocol::{FilePickerSelection, SelectedFile, SelectedFileMetadata};

    let dom = dom::parse_with_scripting(
        r#"<body><form><input id=upload name=upload type=file style="display:none" required>
        <label for=upload>Attach file</label><script>
            const input = document.getElementById('upload');
            const events = [];
            document.body.setAttribute('data-private-hook', typeof __receiveFilePickerUpdate);
            globalThis.__receiveFilePickerUpdate = () => { throw Error('page reply spoof'); };
            input.addEventListener('input', event => {
                const file = input.files[0];
                events.push('input:' + event.isTrusted + ':' + file.name);
                file.text().then(text => document.body.setAttribute('data-bytes', text));
            });
            input.addEventListener('change', event => {
                events.push('change:' + event.isTrusted + ':' + input.value);
                document.body.setAttribute('data-form', String(
                    new FormData(input.form).get('upload') === input.files[0]));
                document.body.setAttribute('data-events', events.join('|'));
            });
            input.addEventListener('cancel', event => {
                events.push('cancel:' + event.isTrusted + ':' + input.files.length);
                document.body.setAttribute('data-events', events.join('|'));
            });
            input.click();
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&file_input_script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.file_picker_actions.is_empty());
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-private-hook").as_deref(), Some("undefined"));

    let label = dom.elements_named("label").next().unwrap();
    let activate = || UserInputEvent::Pointer {
        target: Some(label.clone()),
        phase: "activate",
        button: 0,
        buttons: 0,
        x: 10.0,
        y: 10.0,
        activate: true,
        modifiers: UserInputModifiers::default(),
    };
    let request = runtime.dispatch_user_input(activate());
    assert!(
        request.outcome.errors.is_empty(),
        "{:?}",
        request.outcome.errors
    );
    assert_eq!(request.outcome.file_picker_actions.len(), 1);
    let action = &request.outcome.file_picker_actions[0];
    assert_eq!(
        action.node,
        dom.elements_named("input").next().unwrap().id()
    );
    assert!(!action.multiple);

    let delivered = runtime.deliver_file_picker_selection(
        action.request_id,
        FilePickerSelection::Selected(vec![SelectedFile {
            metadata: SelectedFileMetadata {
                name: "note.txt".into(),
                mime_type: "text/plain".into(),
                last_modified: 1234,
                size: 5,
            },
            bytes: b"hello".to_vec(),
        }]),
    );
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(body.attr("data-bytes").as_deref(), Some("hello"));
    assert_eq!(body.attr("data-form").as_deref(), Some("true"));
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("input:true:note.txt|change:true:C:\\fakepath\\note.txt")
    );

    let again = runtime.dispatch_user_input(activate());
    assert_eq!(again.outcome.file_picker_actions.len(), 1);
    let canceled = runtime.deliver_file_picker_selection(
        again.outcome.file_picker_actions[0].request_id,
        FilePickerSelection::Canceled,
    );
    assert!(canceled.errors.is_empty(), "{:?}", canceled.errors);
    assert_eq!(
        body.attr("data-events").as_deref(),
        Some("input:true:note.txt|change:true:C:\\fakepath\\note.txt|cancel:true:1")
    );
}

#[test]
fn scripted_file_click_inside_real_keyboard_activation_can_request_picker() {
    let dom = dom::parse_with_scripting(
        r#"<body><button id=open>Open</button><input id=upload type=file multiple
        accept="image/png,text/plain" style="display:none">
        <script>
            document.getElementById('open').addEventListener('keydown', () =>
                document.getElementById('upload').click());
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&file_input_script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let outcome = runtime.dispatch_user_input(UserInputEvent::Keyboard {
        target: Some(dom.elements_named("button").next().unwrap()),
        phase: "down",
        key: "x".into(),
        code: "KeyX".into(),
        key_code: 88,
        repeat: false,
        modifiers: UserInputModifiers::default(),
    });
    assert!(
        outcome.outcome.errors.is_empty(),
        "{:?}",
        outcome.outcome.errors
    );
    assert_eq!(outcome.outcome.file_picker_actions.len(), 1);
    assert!(outcome.outcome.file_picker_actions[0].multiple);
    assert_eq!(
        outcome.outcome.file_picker_actions[0].accept,
        "image/png,text/plain"
    );
}

#[test]
fn file_show_picker_requires_activation_and_checks_mutability_first() {
    let dom = dom::parse_with_scripting(
        r#"<body><button id=open>Open</button><input id=upload type=file style="display:none">
        <input id=disabled type=file disabled><script>
            const input = document.getElementById('upload');
            const disabled = document.getElementById('disabled');
            try { input.showPicker(); }
            catch (error) { document.body.setAttribute('data-outside', error.name); }
            try { disabled.showPicker(); }
            catch (error) { document.body.setAttribute('data-disabled', error.name); }
            document.getElementById('open').addEventListener('keydown', () => {
                input.showPicker();
                try { input.showPicker(); }
                catch (error) { document.body.setAttribute('data-consumed', error.name); }
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&file_input_script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.file_picker_actions.is_empty());
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-outside").as_deref(),
        Some("NotAllowedError")
    );
    assert_eq!(
        body.attr("data-disabled").as_deref(),
        Some("InvalidStateError")
    );
    let outcome = runtime.dispatch_user_input(UserInputEvent::Keyboard {
        target: Some(dom.elements_named("button").next().unwrap()),
        phase: "down",
        key: "x".into(),
        code: "KeyX".into(),
        key_code: 88,
        repeat: false,
        modifiers: UserInputModifiers::default(),
    });
    assert!(
        outcome.outcome.errors.is_empty(),
        "{:?}",
        outcome.outcome.errors
    );
    assert_eq!(outcome.outcome.file_picker_actions.len(), 1);
    assert_eq!(
        body.attr("data-consumed").as_deref(),
        Some("NotAllowedError")
    );
}

#[test]
fn same_task_reset_or_type_change_cancels_queued_file_picker() {
    let dom = dom::parse_with_scripting(
        r#"<body><form id=form><input id=upload type=file></form><button id=open>Open</button>
        <script>
            const input = document.getElementById('upload');
            let count = 0;
            document.getElementById('open').addEventListener('keydown', () => {
                input.click();
                if (++count === 1) {
                    document.getElementById('form').reset();
                    try { input.showPicker(); }
                    catch (error) { document.body.setAttribute('data-consumed', error.name); }
                } else input.type = 'text';
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&file_input_script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let button = dom.elements_named("button").next().unwrap();
    let press = || UserInputEvent::Keyboard {
        target: Some(button.clone()),
        phase: "down",
        key: "x".into(),
        code: "KeyX".into(),
        key_code: 88,
        repeat: false,
        modifiers: UserInputModifiers::default(),
    };
    let reset = runtime.dispatch_user_input(press());
    assert!(
        reset.outcome.errors.is_empty(),
        "{:?}",
        reset.outcome.errors
    );
    assert!(reset.outcome.file_picker_actions.is_empty());
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-consumed")
            .as_deref(),
        Some("NotAllowedError")
    );
    let type_change = runtime.dispatch_user_input(press());
    assert!(
        type_change.outcome.errors.is_empty(),
        "{:?}",
        type_change.outcome.errors
    );
    assert!(type_change.outcome.file_picker_actions.is_empty());
}

#[test]
fn file_input_rejects_non_filelist_assignment_without_losing_selection() {
    let (dom, outcome) = execute_html(
        r#"<body><input id=upload type=file><p id=status></p><script>
            const input = document.getElementById('upload');
            const transfer = new DataTransfer();
            transfer.items.add(new File(['x'], 'safe.txt'));
            input.files = transfer.files;
            let rejected = false;
            try { input.files = [new File(['y'], 'other.txt')]; }
            catch (error) { rejected = error instanceof TypeError; }
            document.getElementById('status').textContent = String(rejected &&
                input.files.length === 1 && input.files[0].name === 'safe.txt');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
}

fn file_input_script_inputs(dom: &dom::Dom) -> Vec<ScriptInput> {
    dom.elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect()
}
