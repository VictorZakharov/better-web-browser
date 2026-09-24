use super::network::{pending_runtime, test_response};
use crate::engine::script::{ScriptFetchAction, ScriptFetchEvent};
use base64::Engine as _;
use sha2::{Digest, Sha256, Sha384, Sha512};

#[test]
fn fetch_integrity_withholds_response_until_all_chunks_match() {
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"abcd"));
    let code = format!(
        "fetch('/data', {{ integrity: 'sha256-{digest}' }}).then(r => r.text()).then(text => document.querySelector('div').textContent = text, () => document.querySelector('div').textContent = 'error')"
    );
    let (dom, mut runtime, id) = pending_runtime(&code);
    let head = runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    assert!(head.errors.is_empty(), "{:?}", head.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
    let first =
        runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"ab".to_vec()), None);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(
        first
            .fetch_actions
            .iter()
            .any(|action| matches!(action, ScriptFetchAction::Consume { total: 2, .. }))
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"cd".to_vec()), None);
    let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(end.errors.is_empty(), "{:?}", end.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "abcd"
    );
}

#[test]
fn fetch_integrity_rejects_mismatch_without_exposing_response() {
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(b"expected"));
    let code = format!(
        "fetch('/data', {{ integrity: 'sha384-{digest}' }}).then(() => document.querySelector('div').textContent = 'exposed', () => document.querySelector('div').textContent = 'rejected')"
    );
    let (dom, mut runtime, id) = pending_runtime(&code);
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"wrong".to_vec()), None);
    let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(end.errors.is_empty(), "{:?}", end.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "rejected"
    );
}

#[test]
fn fetch_integrity_does_not_accept_a_good_weak_hash_when_strong_hash_disagrees() {
    let weak = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"payload"));
    let strong = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(b"other"));
    let code = format!(
        "fetch('/data', {{ integrity: 'sha256-{weak} sha512-{strong}' }}).then(\
         () => document.querySelector('div').textContent='accepted',\
         () => document.querySelector('div').textContent='rejected')"
    );
    let (dom, mut runtime, id) = pending_runtime(&code);
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"payload".to_vec()), None);
    let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(end.errors.is_empty(), "{:?}", end.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "rejected"
    );
}

#[test]
fn fetch_integrity_rejects_an_opaque_response_even_when_its_bytes_match() {
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"payload"));
    let code = format!(
        "fetch('/data', {{ integrity: 'sha256-{digest}' }}).then(\
         () => document.querySelector('div').textContent='accepted',\
         () => document.querySelector('div').textContent='rejected')"
    );
    let (dom, mut runtime, id) = pending_runtime(&code);
    let mut response = test_response(b"");
    response.response_type = crate::fetch::ResponseType::Opaque;
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Head(Ok(response)), None);
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(b"payload".to_vec()), None);
    let end = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(end.errors.is_empty(), "{:?}", end.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "rejected"
    );
}
