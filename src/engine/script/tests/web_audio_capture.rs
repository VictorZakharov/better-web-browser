use super::*;
use crate::renderer_protocol::{
    DocumentId, MediaCaptureEvent, MediaCaptureFrame, MediaCaptureFrameKind, MediaCaptureUpdate,
};
use std::time::Duration;

fn script(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.com/#capture-audio".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }
}

fn evaluate(runtime: &mut ScriptRuntime, node: &NodeRef, code: &str) {
    let result = runtime.execute_additional_with_loader(&[script(node, code)], None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

fn create_capture(runtime: &mut ScriptRuntime, node: &NodeRef, rate: u32) -> u64 {
    let initial = runtime.execute_initial(&[script(
        node,
        &format!(
            r#"navigator.mediaDevices.getUserMedia({{audio: true}}).then(stream => {{
                window.stream = stream;
                window.context = new AudioContext({{sampleRate: {rate}}});
                window.source = context.createMediaStreamSource(stream);
                if (!(source instanceof MediaStreamAudioSourceNode) ||
                    source.mediaStream !== stream || source.numberOfInputs !== 0 ||
                    source.numberOfOutputs !== 1)
                    throw Error('capture source interface is incorrect');
                let missingAudio = false;
                try {{ context.createMediaStreamSource(new MediaStream()); }}
                catch (error) {{ missingAudio = error.name === 'InvalidStateError'; }}
                if (!missingAudio) throw Error('audio-less stream was accepted');
            }});"#
        ),
    )]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.media_device_actions.len(), 1);
    let id = initial.media_device_actions[0].request_id;
    let started = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(started.errors.is_empty(), "{:?}", started.errors);
    assert!(started.graph_audio_actions.is_empty());
    id
}

fn packet(id: u64, sequence: u64, rate: u32, samples: &[i16]) -> MediaCaptureFrame {
    MediaCaptureFrame {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        track_id: 2,
        sequence,
        timestamp_100ns: (sequence - 1) * samples.len() as u64 * 10_000_000 / rate as u64,
        kind: MediaCaptureFrameKind::AudioPcm16,
        width_or_rate: rate,
        height_or_frames: samples.len() as u32,
        stride_or_channels: 1,
        bytes: samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect(),
    }
}

fn deliver_packet(runtime: &mut ScriptRuntime, frame: MediaCaptureFrame) {
    frame.validate().unwrap();
    let result = runtime.deliver_media_capture_audio_frame(frame);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}

fn next_pcm(runtime: &mut ScriptRuntime) -> (u32, Vec<u8>) {
    for _ in 0..10 {
        let result = runtime.advance_time(Duration::from_millis(10), 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        if let Some(action) = result.graph_audio_actions.into_iter().next() {
            let ScriptGraphAudioAction::Queue { stream_id, pcm, .. } = action else {
                panic!("expected graph PCM, got {action:?}");
            };
            return (stream_id, pcm);
        }
    }
    panic!("capture audio did not produce a graph chunk");
}

fn sample(pcm: &[u8], frame: usize, channel: usize) -> i16 {
    let index = (frame * 2 + channel) * 2;
    i16::from_le_bytes([pcm[index], pcm[index + 1]])
}

#[test]
fn microphone_is_silent_until_connected_to_existing_graph() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let id = create_capture(&mut runtime, &node, 16_000);
    deliver_packet(&mut runtime, packet(id, 1, 8_000, &[8_192; 128]));
    runtime.set_audio_activation(true);
    evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    let (stream_id, silent) = next_pcm(&mut runtime);
    assert!(
        silent.iter().all(|byte| *byte == 0),
        "unconnected mic echoed to speaker"
    );

    evaluate(
        &mut runtime,
        &node,
        &format!(
            "__receiveAudioGraphStatus({stream_id}, 'accepted'); source.connect(context.destination);"
        ),
    );
    deliver_packet(&mut runtime, packet(id, 2, 8_000, &[4_096; 128]));
    let (_, audible) = next_pcm(&mut runtime);
    assert_eq!(
        sample(&audible, 0, 0),
        4_096,
        "connecting a source replayed audio captured before connection"
    );
    assert_eq!(sample(&audible, 0, 1), 4_096);
}

#[test]
fn microphone_source_resamples_and_keeps_its_selected_track_after_removal() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let id = create_capture(&mut runtime, &node, 16_000);
    evaluate(
        &mut runtime,
        &node,
        r#"
        source.connect(context.destination);
        stream.removeTrack(stream.getAudioTracks()[0]);
        if (stream.getAudioTracks().length !== 0 || source.mediaStream !== stream)
            throw Error('selected source track changed with stream membership');
    "#,
    );
    let ramp = (0..128).map(|frame| frame * 128).collect::<Vec<i16>>();
    deliver_packet(&mut runtime, packet(id, 1, 8_000, &ramp));
    runtime.set_audio_activation(true);
    evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    let (_, pcm) = next_pcm(&mut runtime);
    assert_eq!(sample(&pcm, 0, 0), 0);
    assert_eq!(sample(&pcm, 1, 0), 64);
    assert_eq!(sample(&pcm, 2, 0), 128);
    assert_eq!(sample(&pcm, 1, 1), 64);
}

#[test]
fn microphone_ring_is_bounded_and_disabling_or_stopping_clears_buffered_pcm() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let id = create_capture(&mut runtime, &node, 8_000);
    evaluate(&mut runtime, &node, "source.connect(context.destination);");
    for sequence in 1..=12 {
        deliver_packet(
            &mut runtime,
            packet(id, sequence, 8_000, &[(sequence * 1_000) as i16; 128]),
        );
    }
    deliver_packet(&mut runtime, packet(id, 10, 8_000, &[30_000; 128]));
    runtime.set_audio_activation(true);
    evaluate(&mut runtime, &node, "__notifyAudioActivation();");
    let (stream_id, bounded) = next_pcm(&mut runtime);
    assert_eq!(
        sample(&bounded, 0, 0),
        5_000,
        "a full ring replayed stale microphone packets"
    );

    evaluate(
        &mut runtime,
        &node,
        &format!(
            "stream.getAudioTracks()[0].enabled = false; __receiveAudioGraphStatus({stream_id}, 'accepted');"
        ),
    );
    let (stream_id, disabled) = next_pcm(&mut runtime);
    assert!(
        disabled.iter().all(|byte| *byte == 0),
        "disabled mic leaked queued speech"
    );

    evaluate(
        &mut runtime,
        &node,
        &format!(
            "stream.getAudioTracks()[0].stop(); __receiveAudioGraphStatus({stream_id}, 'accepted');"
        ),
    );
    let (_, ended) = next_pcm(&mut runtime);
    assert!(
        ended.iter().all(|byte| *byte == 0),
        "stopped mic leaked queued speech"
    );
}
