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
            const snapshot = transfer.files;
            input.files = snapshot;
            transfer.items.remove(0);
            const entries = new FormData(document.getElementById('form')).getAll('photos');
            const selected = input.files === snapshot && input.files.length === 2 &&
                input.files.item(0) === first && input.files.item(1) === second &&
                input.value === 'C:\\fakepath\\one.png' &&
                entries.length === 2 && entries[0] === first && entries[1] === second &&
                input.checkValidity();
            input.files = null;
            document.getElementById('status').textContent = String(selected &&
                input.files.length === 0 && input.validity.valueMissing);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "true"
    );
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
