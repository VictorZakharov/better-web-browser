use super::*;
use crate::renderer_protocol::{
    DocumentId, SpeechAction, SpeechEvent, SpeechUpdate, SpeechVoiceInfo,
};

#[test]
fn speech_queue_dispatches_native_requests_and_browser_events() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const utterance = new SpeechSynthesisUtterance('Hello from Breeze');
            utterance.onstart = () => document.body.setAttribute('data-start', 'yes');
            utterance.onend = () => document.body.setAttribute('data-end', 'yes');
            speechSynthesis.speak(utterance);
            document.body.setAttribute('data-speaking', String(speechSynthesis.speaking));
            document.body.setAttribute('data-pending', String(speechSynthesis.pending));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let speech = initial
        .speech_actions
        .iter()
        .find(|action| matches!(action.action, SpeechAction::Speak { .. }))
        .unwrap();
    let id = speech.utterance_id;
    match &speech.action {
        SpeechAction::Speak {
            text,
            rate,
            pitch,
            volume,
            ..
        } => {
            assert_eq!(text, "Hello from Breeze");
            assert_eq!((*rate, *pitch, *volume), (1.0, 1.0, 1.0));
        }
        _ => unreachable!(),
    }
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-speaking")
            .as_deref(),
        Some("false")
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-pending")
            .as_deref(),
        Some("true")
    );
    let document = DocumentId::new(1).unwrap();
    for event in [SpeechEvent::Started, SpeechEvent::Ended] {
        let delivered = runtime.deliver_speech_update(SpeechUpdate {
            document,
            utterance_id: id,
            event,
        });
        assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    }
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-start").as_deref(), Some("yes"));
    assert_eq!(body.attr("data-end").as_deref(), Some("yes"));
}

#[test]
fn consecutive_speak_calls_submit_both_requests_in_the_same_activation_task() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            for (const text of ['first', 'second']) {
                const utterance = new SpeechSynthesisUtterance(text);
                utterance.onend = () => document.body.setAttribute('data-last-end', text);
                speechSynthesis.speak(utterance);
            }
            document.body.setAttribute('data-pending', String(speechSynthesis.pending));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let speech = initial
        .speech_actions
        .iter()
        .filter(|action| matches!(action.action, SpeechAction::Speak { .. }))
        .collect::<Vec<_>>();
    assert_eq!(speech.len(), 2);
    assert_ne!(speech[0].utterance_id, speech[1].utterance_id);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-pending")
            .as_deref(),
        Some("true")
    );
    for action in speech {
        let delivered = runtime.deliver_speech_update(SpeechUpdate {
            document: DocumentId::new(1).unwrap(),
            utterance_id: action.utterance_id,
            event: SpeechEvent::Ended,
        });
        assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    }
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-last-end")
            .as_deref(),
        Some("second")
    );
}

#[test]
fn installed_voice_update_populates_get_voices_and_fires_change_event() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            speechSynthesis.onvoiceschanged = () => {
                const voices = speechSynthesis.getVoices();
                document.body.setAttribute('data-voice', voices[0]?.name || 'none');
            };
            speechSynthesis.getVoices();
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request = initial
        .speech_actions
        .iter()
        .find(|action| matches!(action.action, SpeechAction::GetVoices))
        .unwrap();
    let delivered = runtime.deliver_speech_update(SpeechUpdate {
        document: DocumentId::new(1).unwrap(),
        utterance_id: request.utterance_id,
        event: SpeechEvent::Voices(vec![SpeechVoiceInfo {
            voice_uri: "native:test".into(),
            name: "Test Voice".into(),
            lang: "en-US".into(),
            is_default: true,
        }]),
    });
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-voice")
            .as_deref(),
        Some("Test Voice")
    );
}

#[test]
fn invalid_utterance_does_not_send_native_speech() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const utterance = new SpeechSynthesisUtterance('test');
            utterance.volume = 2;
            let rejected = false;
            try { speechSynthesis.speak(utterance); }
            catch (error) { rejected = error instanceof RangeError; }
            document.body.setAttribute('data-rejected', String(rejected));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(
        !initial
            .speech_actions
            .iter()
            .any(|action| matches!(action.action, SpeechAction::Speak { .. }))
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-rejected")
            .as_deref(),
        Some("true")
    );
}

#[test]
fn idle_documents_do_not_emit_speech_ipc() {
    let dom = dom::parse_with_scripting(
        "<body><script>document.title = 'idle'</script></body>",
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.speech_actions.is_empty());
}

#[test]
fn browser_owned_speech_objects_follow_constructor_and_singleton_idl() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const utterance = new SpeechSynthesisUtterance('test');
            let synthesisBlocked = false, voiceBlocked = false, eventBlocked = false;
            let errorBlocked = false;
            try { new SpeechSynthesis(); } catch (e) { synthesisBlocked = e instanceof TypeError; }
            try { new SpeechSynthesisVoice({}); } catch (e) { voiceBlocked = e instanceof TypeError; }
            try { new SpeechSynthesisEvent('start'); } catch (e) { eventBlocked = e instanceof TypeError; }
            try { new SpeechSynthesisErrorEvent('error', { utterance }); }
            catch (e) { errorBlocked = e instanceof TypeError; }
            const singleton = speechSynthesis;
            speechSynthesis = {};
            const valid = new SpeechSynthesisEvent('start', { utterance });
            document.body.setAttribute('data-idl', [
                synthesisBlocked, voiceBlocked, eventBlocked, errorBlocked,
                speechSynthesis === singleton, valid.utterance === utterance
            ].join(','));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-idl")
            .as_deref(),
        Some("true,true,true,true,true,true")
    );
    assert!(initial.speech_actions.is_empty());
}

#[test]
fn rejected_audio_reports_not_allowed_without_leaving_a_pending_utterance() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const utterance = new SpeechSynthesisUtterance('test');
            utterance.onerror = event => document.body.setAttribute('data-error', event.error);
            speechSynthesis.speak(utterance);
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let id = initial.speech_actions[0].utterance_id;
    let delivered = runtime.deliver_speech_update(SpeechUpdate {
        document: DocumentId::new(1).unwrap(),
        utterance_id: id,
        event: SpeechEvent::Error("not-allowed".into()),
    });
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error")
            .as_deref(),
        Some("not-allowed")
    );
}
