use super::network::{pending_runtime, test_response};
use super::*;

#[test]
fn xhr_reopen_does_not_deliver_the_old_abort_to_the_new_request() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        const errors = [];
        xhr.onabort = () => errors.push('abort');
        xhr.onerror = () => errors.push('error');
        xhr.onload = () => document.querySelector('div').textContent =
            xhr.status + '|' + xhr.responseText + '|' + errors.join(',');
        xhr.open('GET', '/old'); xhr.send();
        xhr.open('GET', '/new'); xhr.send();"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|new|"
    );
}

#[test]
fn xhr_abort_handler_can_start_a_replacement_without_resetting_its_state() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        let aborts = 0;
        xhr.onabort = () => {
            aborts++;
            if (aborts === 1) { xhr.open('GET', '/new'); xhr.send(); }
        };
        xhr.onload = () => document.querySelector('div').textContent =
            xhr.status + '|' + xhr.responseText + '|' + aborts + '|' + afterAbort;
        xhr.open('GET', '/old'); xhr.send(); xhr.abort();
        const afterAbort = xhr.readyState;"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|new|1|1"
    );
}

#[test]
fn xhr_reopen_during_response_headers_does_not_read_the_old_body() {
    replacement_during_response("xhr.onreadystatechange", "xhr.readyState === 2");
}

#[test]
fn xhr_reopen_during_progress_does_not_complete_or_error_the_new_request() {
    replacement_during_response("xhr.onprogress", "true");
}

fn replacement_during_response(handler: &str, condition: &str) {
    let script = format!(
        r#"const xhr = new XMLHttpRequest();
        let replaced = false;
        const errors = [];
        xhr.onabort = () => errors.push('abort');
        xhr.onerror = () => errors.push('error');
        {handler} = () => {{
            if (!replaced && ({condition})) {{
                replaced = true;
                xhr.open('GET', '/new'); xhr.send();
            }}
        }};
        xhr.onload = () => document.querySelector('div').textContent =
            xhr.status + '|' + xhr.responseText + '|' + errors.join(',');
        xhr.open('GET', '/old'); xhr.send();"#
    );
    let (dom, mut runtime, id) = pending_runtime(&script);
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"old")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let replacement = outcome
        .fetch_actions
        .iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, request } if request.url.as_str().ends_with("/new") => {
                Some(*id)
            }
            _ => None,
        })
        .expect("replacement request must start");
    let outcome = runtime.complete_fetch_with_loader(replacement, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|new|"
    );
}

#[test]
fn xhr_abort_from_loadstart_neither_launches_fetch_nor_throws() {
    let (dom, outcome) = execute_html(
        r#"<div></div><script>
        const xhr = new XMLHttpRequest();
        xhr.onloadstart = () => xhr.abort();
        xhr.open('GET', '/old');
        xhr.send();
        document.querySelector('div').textContent = xhr.readyState + '|' + xhr.status;
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.fetch_actions.is_empty(),
        "{:?}",
        outcome.fetch_actions
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "0|0"
    );
}

#[test]
fn xhr_reopen_from_loadstart_starts_only_the_replacement() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        let replaced = false;
        xhr.onloadstart = () => {
            if (!replaced) {
                replaced = true;
                xhr.open('GET', '/new'); xhr.send();
            }
        };
        xhr.onload = () => document.querySelector('div').textContent = xhr.responseText;
        xhr.open('GET', '/old'); xhr.send();"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.fetch_actions.is_empty());
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "new"
    );
}

#[test]
fn xhr_loading_state_waits_for_body_bytes_and_empty_responses_skip_it() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        const states = [];
        xhr.onreadystatechange = () => {
            states.push(xhr.readyState);
            document.querySelector('div').textContent = states.join(',');
        };
        xhr.open('GET', '/empty'); xhr.send();"#,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "1,2"
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "1,2,4"
    );
}

#[test]
fn xhr_response_states_do_not_consult_an_author_replacement_constructor() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        window.XMLHttpRequest = { HEADERS_RECEIVED: 80, LOADING: 90 };
        const states = [];
        xhr.onreadystatechange = () => states.push(xhr.readyState);
        xhr.onload = () => document.querySelector('div').textContent = states.join(',');
        xhr.open('GET', '/new'); xhr.send();"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "1,2,3,4"
    );
}

#[test]
fn xhr_upload_loadstart_can_cancel_without_starting_fetch() {
    let (dom, outcome) = execute_html(
        r#"<div></div><script>
        const xhr = new XMLHttpRequest();
        xhr.upload.onloadstart = () => xhr.abort();
        xhr.open('POST', '/old'); xhr.send('payload');
        document.querySelector('div').textContent = xhr.readyState + '|' + xhr.status;
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.fetch_actions.is_empty(),
        "{:?}",
        outcome.fetch_actions
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "0|0"
    );
}

#[test]
fn xhr_upload_error_can_restart_without_old_error_steps_resetting_the_new_request() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const xhr = new XMLHttpRequest();
        xhr.upload.onerror = () => { xhr.open('GET', '/new'); xhr.send(); };
        xhr.onload = () => document.querySelector('div').textContent = xhr.status + '|' + xhr.responseText;
        xhr.open('POST', '/old'); xhr.send('payload');"#,
    );
    let outcome = runtime.complete_fetch_with_loader(
        id,
        Err(crate::fetch::FetchError::new(
            crate::fetch::FetchErrorKind::Network,
            "fixture upload failure",
        )),
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let replacement = outcome
        .fetch_actions
        .iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, request } if request.url.as_str().ends_with("/new") => {
                Some(*id)
            }
            _ => None,
        })
        .expect("upload error callback must start replacement");
    let outcome = runtime.complete_fetch_with_loader(replacement, Ok(test_response(b"new")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "200|new"
    );
}
