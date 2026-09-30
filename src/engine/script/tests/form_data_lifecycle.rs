use super::*;

#[test]
fn formdata_event_uses_provisional_entries_and_constructor_gets_a_clone() {
    let (dom, outcome) = execute_html(
        r#"<body><form id=form><input name=first value=one></form><output></output><script>
            const form = document.getElementById('form');
            let eventData;
            form.addEventListener('formdata', event => {
                eventData = event.formData;
                event.formData.append('during', 'yes');
                event.formData.set('first', 'changed');
            });
            const constructed = new FormData(form);
            const inEvent = eventData !== constructed &&
                constructed.get('during') === 'yes' && constructed.get('first') === 'changed';
            eventData.set('first', 'later');
            eventData.append('after', 'not copied');
            constructed.append('own', 'separate');
            document.querySelector('output').textContent = String(inEvent &&
                constructed.get('first') === 'changed' && !constructed.has('after') &&
                !eventData.has('own'));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn formdata_entries_and_native_iterators_do_not_use_author_visible_storage() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const data = new FormData();
            data.append('first', '1');
            data.append('second', '2');
            data.__entries = [['spoof', '3']];
            data.entries = () => { throw Error('author entries'); };
            const names = [...FormData.prototype.keys.call(data)].join(',');
            const values = [...FormData.prototype.values.call(data)].join(',');
            const pairs = [...data].map(([name, value]) => name + value).join(',');
            let invalid = false;
            try { FormData.prototype.get.call({}, 'first'); }
            catch (error) { invalid = error instanceof TypeError; }
            document.querySelector('output').textContent = String(
                names === 'first,second' && values === '1,2' &&
                pairs === 'first1,second2' && invalid &&
                data.get('first') === '1' && !data.has('spoof') &&
                !Object.prototype.hasOwnProperty.call(new FormData(), '__entries'));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn filename_overload_requires_blob_and_copies_private_file_bytes() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const data = new FormData();
            let appendError = false, setError = false;
            try { data.append('x', 'text', 'name.txt'); }
            catch (error) { appendError = error instanceof TypeError; }
            try { data.set('x', 'text', 'name.txt'); }
            catch (error) { setError = error instanceof TypeError; }
            const source = new Blob(['private bytes'], {type:'text/plain'});
            Object.defineProperty(source, 'type', {get() { throw Error('author type'); }});
            data.append('file', source, 'chosen.txt');
            const file = data.get('file');
            file.text().then(text => document.querySelector('output').textContent = String(
                appendError && setError && data.get('x') === null &&
                file instanceof File && file.name === 'chosen.txt' &&
                file.type === 'text/plain' && text === 'private bytes'));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn formdata_mutation_during_iteration_reads_current_private_list() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const data = new FormData();
            data.append('a', '1');
            data.append('b', '2');
            const visited = [];
            data.forEach((value, name) => {
                visited.push(name + value);
                if (name === 'a') {
                    data.delete('b');
                    data.append('c', '3');
                }
            });
            document.querySelector('output').textContent = String(
                visited.join(',') === 'a1,c3' && [...data.keys()].join(',') === 'a,c');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}
