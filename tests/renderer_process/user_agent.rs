use super::support::*;
use better_web_browser::branding::UserAgentMode;
use better_web_browser::renderer_process::RendererSession;

#[test]
fn selected_identity_reaches_each_isolated_document_realm() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (index, mode) in [
        UserAgentMode::Breeze,
        UserAgentMode::Chrome,
        UserAgentMode::Firefox,
    ]
    .into_iter()
    .enumerate()
    {
        let mut launch = options();
        launch.user_agent_mode = mode;
        let mut session = RendererSession::launch(launch).expect("launch renderer");
        let presentation = load_html_document(
            &session,
            200 + index as u64,
            "<!doctype html><script>document.title=navigator.userAgent</script>",
        );
        assert_eq!(presentation.title, mode.user_agent());
        session.shutdown().expect("shutdown renderer");
    }
}
