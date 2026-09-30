//! Actual JavaScript Blob output, independently demuxed and decoded after stop.

use super::*;
use crate::opus_audio::{Limits, Stream};
use ogg::reading::PacketReader;
use std::sync::Arc;
use std::time::Instant;

mod chunks;
mod lifecycle;
mod options;

fn start(options: &str, start_call: &str) -> (dom::Dom, ScriptRuntime, u64) {
    let html = format!(
        r#"<body><script>
        window.parts = []; window.events = []; window.timecodes = [];
        navigator.mediaDevices.getUserMedia({{audio: true}}).then(stream => {{
            window.stream = stream; window.track = stream.getAudioTracks()[0];
            window.recorder = new MediaRecorder(stream, {options});
            recorder.onstart = () => events.push('start');
            recorder.onpause = () => events.push('pause');
            recorder.onresume = () => events.push('resume');
            recorder.onerror = event => events.push(event.error.name);
            recorder.ondataavailable = event => {{
                if (!event.isTrusted || !(event instanceof BlobEvent)) throw Error('Blob event');
                events.push('data'); parts.push(event.data); timecodes.push(event.timecode);
                document.body.setAttribute('data-type', event.data.type);
            }};
            recorder.onstop = () => {{
                events.push('stop');
                new Blob(parts).bytes().then(bytes => {{
                    document.body.setAttribute('data-opus', btoa(String.fromCharCode(...bytes)));
                    document.body.setAttribute('data-events', events.join(','));
                    document.body.setAttribute('data-parts', String(parts.length));
                    document.body.setAttribute('data-timecodes', timecodes.join(','));
                    document.body.setAttribute('data-state', recorder.state);
                }});
            }};
            {start_call}
        }});
        </script></body>"#
    );
    let dom = dom::parse_with_scripting(&html, true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: false,
            microphone: true,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    run_queued_events(&mut runtime);
    (dom, runtime, request_id)
}

fn run(dom: &dom::Dom, runtime: &mut ScriptRuntime, code: &str) {
    let node = dom.elements_named("script").next().unwrap();
    let outcome = runtime
        .execute_additional_with_loader(&[input(&node, "recorder-operation.js", code, true)], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    run_queued_events(runtime);
}

fn capture(
    runtime: &mut ScriptRuntime,
    request_id: u64,
    sequence: u64,
    rate: u32,
    channels: u32,
    frames: u32,
) {
    let samples = (0..frames).flat_map(|frame| {
        (0..channels).map(move |channel| {
            let frequency = if channel == 0 { 440.0 } else { 660.0 };
            let index = (sequence - 1) * u64::from(rate / 50) + u64::from(frame);
            let phase = index as f64 * frequency * std::f64::consts::TAU / f64::from(rate);
            (phase.sin() * 16_000.0).round() as i16
        })
    });
    let outcome = runtime.deliver_media_capture_audio_frame(MediaCaptureFrame {
        document: DocumentId::new(1).unwrap(),
        request_id,
        track_id: 2,
        sequence,
        timestamp_100ns: (sequence - 1) * 200_000,
        kind: MediaCaptureFrameKind::AudioPcm16,
        width_or_rate: rate,
        height_or_frames: frames,
        stride_or_channels: channels,
        bytes: samples.flat_map(i16::to_le_bytes).collect(),
    });
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

fn attribute(dom: &dom::Dom, name: &str) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr(name)
        .unwrap()
}

fn bytes(dom: &dom::Dom) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(attribute(dom, "data-opus"))
        .unwrap()
}

fn decoded(dom: &dom::Dom, channels: u16, frames: u64) -> Vec<f32> {
    let source: Arc<[u8]> = bytes(dom).into();
    let mut reader = PacketReader::new(Cursor::new(Arc::clone(&source)));
    let head = reader.read_packet().unwrap().unwrap();
    assert!(head.first_in_stream());
    assert_eq!(&head.data[..8], b"OpusHead");
    assert_eq!(u16::from(head.data[9]), channels);
    let pre_skip = u16::from_le_bytes(head.data[10..12].try_into().unwrap());
    assert!(pre_skip > 0, "the encoder lookahead must be represented");
    assert_eq!(
        &head.data[16..19],
        &[0, 0, 0],
        "zero gain, mapping family 0"
    );
    let serial = head.stream_serial();
    assert!(
        reader
            .read_packet()
            .unwrap()
            .unwrap()
            .data
            .starts_with(b"OpusTags")
    );
    let mut ended = false;
    while let Some(packet) = reader.read_packet().unwrap() {
        assert_eq!(packet.stream_serial(), serial);
        assert!(
            !packet.first_in_stream(),
            "chunks must not restart the Ogg stream"
        );
        assert!(!ended, "no packets after EOS");
        if packet.last_in_stream() {
            assert_eq!(packet.absgp_page(), u64::from(pre_skip) + frames);
            ended = true;
        }
    }
    assert!(ended, "completed recording must have an EOS page");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = Stream::open(source, Limits::default(), None, deadline).unwrap();
    assert_eq!((stream.channels(), stream.frames()), (channels, frames));
    let mut samples = Vec::new();
    while let Some(chunk) = stream.next_pcm(None, deadline).unwrap() {
        assert!(!chunk.is_empty());
        assert!(chunk.iter().all(|sample| sample.is_finite()));
        samples.extend(chunk);
    }
    assert_eq!(samples.len() as u64, frames * u64::from(channels));
    samples
}
