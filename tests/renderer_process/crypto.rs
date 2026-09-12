use super::support::*;
use better_web_browser::renderer_process::RendererSession;

#[test]
fn crypto_randomness_contract_runs_inside_app_container() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let html = format!(
        "<!doctype html><title>pending</title><script>{}\n\
         const result = checkCryptoRandomness();\
         document.title = result.failures.length ? JSON.stringify(result) : 'crypto passed';</script>",
        include_str!("../fixtures/crypto-randomness.js")
    );
    let presentation = load_html_document(&session, 98, &html);
    assert_eq!(presentation.title, "crypto passed");
    session.cancel_document(presentation.document).unwrap();
    session.shutdown().unwrap();
}
