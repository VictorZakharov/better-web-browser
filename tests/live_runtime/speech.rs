use super::*;

const NO_ACTIVATION: &str = r#"<!doctype html><title>speech activation</title>
<style>html,body{margin:0;background:rgb(17,170,34)}</style>
<script>
  const utterance = new SpeechSynthesisUtterance('This must not be spoken');
  utterance.onerror = event => console.log('__SPEECH_REJECTED__' + event.error);
  utterance.onstart = () => console.error('__SPEECH_UNEXPECTED_START__');
  speechSynthesis.speak(utterance);
</script>"#;

#[test]
fn renderer_speech_request_without_trusted_activation_is_rejected_without_audio() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind speech fixture");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| NO_ACTIVATION));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/speech"), &artifacts, 1000);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).expect("read speech benchmark report");
    assert!(
        report.contains("__SPEECH_REJECTED__not-allowed"),
        "{report}"
    );
    assert!(!report.contains("__SPEECH_UNEXPECTED_START__"), "{report}");
    assert_green_capture(&artifacts, "speech rejection changed the document surface");
}
