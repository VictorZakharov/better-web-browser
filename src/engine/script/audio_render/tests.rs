//! The document agent is deliberately idle while the DSP agent drives output.
use super::*;
use crate::engine::{
    dom,
    script::{ScriptFetchOptions, ScriptGraphAudioAction, ScriptInput, ScriptKind, ScriptRuntime},
};
use std::sync::Condvar;
use std::time::Instant;

#[derive(Default)]
struct Capture {
    samples: Mutex<Vec<Vec<u8>>>,
    ready: Condvar,
    epoch: Mutex<Option<Instant>>,
    submitted: AtomicU64,
    closed: Mutex<Vec<u32>>,
}

impl Capture {
    fn wait_chunks(&self, count: usize) -> Vec<Vec<u8>> {
        let samples = self.samples.lock().unwrap();
        let (samples, timeout) = self
            .ready
            .wait_timeout_while(samples, Duration::from_secs(3), |samples| {
                samples.len() < count
            })
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "DSP agent failed to continue without document tasks"
        );
        samples.clone()
    }
}

impl AudioRenderOutput for Capture {
    fn queue(&self, _: u32, _: GraphPcmFormat, pcm: Vec<u8>) -> Result<bool, String> {
        self.epoch.lock().unwrap().get_or_insert_with(Instant::now);
        self.submitted
            .fetch_add(pcm.len() as u64 / 4, Ordering::Release);
        self.samples.lock().unwrap().push(pcm);
        self.ready.notify_all();
        Ok(true)
    }
    fn health(&self, _: u32) -> Option<GraphPcmHealth> {
        let submitted_frames = self.submitted.load(Ordering::Acquire);
        let played_frames = self
            .epoch
            .lock()
            .unwrap()
            .map_or(0, |epoch| (epoch.elapsed().as_secs_f64() * 48000.0) as u64)
            .min(submitted_frames);
        Some(GraphPcmHealth {
            submitted_frames,
            played_frames,
            playing: true,
            ..Default::default()
        })
    }
    fn started(&self, _: u32) -> Result<bool, String> {
        Ok(self.submitted.load(Ordering::Acquire) > 0)
    }
    fn close(&self, stream: u32) {
        self.closed.lock().unwrap().push(stream);
    }
}

#[test]
fn acquired_loop_and_graph_edits_continue_without_document_clock_ticks() {
    let dom = dom::parse_with_scripting("<body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.enable_audio_render_thread();
    runtime.set_audio_activation(true);
    let make_script = |code: &str| ScriptInput {
        source_url: "https://example.com/audio.js".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let initial = runtime.execute_initial(&[make_script(
        "window.context = new AudioContext(); window.gain = context.createGain(); gain.gain.value = .5;
         const buffer = context.createBuffer(1, 1024, 48000);
         buffer.getChannelData(0).forEach((_, i, samples) => samples[i] = (i % 64) / 128 - .25);
         const source = context.createBufferSource(); source.buffer = buffer; source.loop = true;
         source.connect(gain).connect(context.destination); source.start();
         buffer.getChannelData(0).fill(.9);")]);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut actions = initial.graph_audio_actions;
    for _ in 0..10 {
        if !actions.is_empty() {
            break;
        }
        let outcome = runtime.advance_time(Duration::from_millis(10), 4);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        actions.extend(outcome.graph_audio_actions);
    }
    let ScriptGraphAudioAction::Render { renderer, .. } = actions.pop().expect("render stream")
    else {
        panic!("native binding used document-thread sample generation");
    };
    let output = Arc::new(Capture::default());
    renderer.attach(output.clone());
    // No runtime.advance_time(), layout callback, status delivery or main-thread
    // sample generation is allowed between attachment and these device chunks.
    let samples = output.wait_chunks(20);
    for chunk in samples.iter().take(20) {
        for (frame, stereo) in chunk.chunks_exact(4).enumerate() {
            let value = ((frame % 64) as f64 / 128.0 - 0.25) * 0.5;
            let expected = (value * if value < 0.0 { 32768.0 } else { 32767.0 }).round() as i16;
            assert_eq!(i16::from_le_bytes([stereo[0], stereo[1]]), expected);
            assert_eq!(stereo[0..2], stereo[2..4]);
        }
    }
    let before = renderer.frame();
    let updated =
        runtime.execute_additional_with_loader(&[make_script("gain.gain.value = 0;")], None);
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    let chunks = output.samples.lock().unwrap().len();
    let samples = output.wait_chunks(chunks + 10);
    assert!(samples.last().unwrap().iter().all(|sample| *sample == 0));
    assert!(
        renderer.frame() > before,
        "graph edits reset the rendering clock"
    );
}
