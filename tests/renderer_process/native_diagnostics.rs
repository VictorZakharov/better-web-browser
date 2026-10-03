use super::{SERIAL, options};
use better_web_browser::renderer_process::{RendererSession, RendererState};
use better_web_browser::renderer_protocol::TestCommand;

#[test]
fn native_stdout_and_stderr_cannot_corrupt_renderer_protocol() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden contained renderer");
    session
        .send_test_command(TestCommand::NativeDiagnostics)
        .unwrap();
    // Native C writes precede this Ping on the same ordered command stream.
    // Before isolation, either stream's non-frame bytes kill the IPC reader.
    session
        .ping(std::time::Duration::from_secs(3))
        .expect("protocol survives native library diagnostics");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session
        .shutdown()
        .expect("clean shutdown after diagnostics");
}
