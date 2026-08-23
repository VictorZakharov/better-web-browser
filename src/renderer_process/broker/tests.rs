use super::*;

#[test]
fn startup_rejects_a_renderer_that_echoes_another_context() {
    let nonce = crate::renderer_protocol::Nonce::new([7; 32]);
    let expected = BrowsingContextId::new(3).unwrap();
    let stale = BrowsingContextId::new(4).unwrap();
    let containment = ContainmentReport {
        app_container: true,
        no_console_window: true,
        minimal_environment: true,
    };

    assert_eq!(
        validate_ready(nonce, nonce, expected, stale, containment),
        Err("renderer returned a stale browsing context".into())
    );
}
