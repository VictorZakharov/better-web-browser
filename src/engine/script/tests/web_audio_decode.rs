use super::*;
use std::time::{Duration, Instant};

fn execute_decode_html_until_logged(html: &str, expected: &[&str]) -> Vec<String> {
    let dom = dom::parse_with_scripting(html, true);
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&scripts);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut console = initial.console;

    // The native decoder runs on a worker. The standalone execution helper's
    // virtual startup horizon cannot guarantee that worker has finished on CI.
    let deadline = Instant::now() + Duration::from_secs(10);
    while expected
        .iter()
        .any(|message| !console.iter().any(|line| line == message))
    {
        assert!(
            Instant::now() < deadline,
            "audio decode did not settle; console: {console:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
        let next = runtime.advance_time(Duration::from_millis(10), 64);
        assert!(next.errors.is_empty(), "{:?}", next.errors);
        console.extend(next.console);
    }
    console
}

#[test]
fn decode_audio_data_detaches_synchronously_and_returns_resampled_pcm() {
    let console = execute_decode_html_until_logged(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 16000);
        const input = new ArrayBuffer(48), view = new DataView(input);
        const ascii = (offset, text) => {
            for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
        };
        ascii(0, 'RIFF'); view.setUint32(4, 40, true); ascii(8, 'WAVE');
        ascii(12, 'fmt '); view.setUint32(16, 16, true);
        view.setUint16(20, 1, true); view.setUint16(22, 1, true);
        view.setUint32(24, 8000, true); view.setUint32(28, 16000, true);
        view.setUint16(32, 2, true); view.setUint16(34, 16, true);
        ascii(36, 'data'); view.setUint32(40, 4, true);
        view.setInt16(44, 0, true); view.setInt16(46, 16384, true);
        const events = [];
        const promise = context.decodeAudioData(input, buffer => {
            events.push('callback');
            if (buffer.sampleRate !== 16000 || buffer.length !== 4 ||
                buffer.numberOfChannels !== 1)
                throw Error('decoded AudioBuffer metadata');
        });
        if (input.byteLength !== 0) throw Error('encoded input was not detached synchronously');
        events.push('returned');
        promise.then(buffer => {
            events.push('then');
            const samples = buffer.getChannelData(0);
            if (samples[0] !== 0 || samples[1] !== 0.25 ||
                samples[2] !== 0.5 || samples[3] !== 0.5 ||
                events.join(',') !== 'returned,callback,then')
                throw Error('decoded PCM or completion ordering: ' + events);
            console.log('WAV decode and detach passed');
        });
    </script>"#,
        &["log: WAV decode and detach passed"],
    );
    assert_eq!(console, ["log: WAV decode and detach passed"]);
}

#[test]
fn decode_audio_data_rejects_unsupported_bytes_and_detached_input() {
    let console = execute_decode_html_until_logged(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        let wrongType = false;
        try { context.decodeAudioData(new Uint8Array(4)); }
        catch (error) { wrongType = error instanceof TypeError; }
        if (!wrongType) throw Error('non-ArrayBuffer input was accepted');

        let callbackError;
        context.decodeAudioData(new ArrayBuffer(4), null, error => {
            callbackError = error;
        }).catch(error => {
            if (error.name !== 'EncodingError' || error !== callbackError)
                throw Error('unsupported bytes should reject with EncodingError');
            console.log('unsupported audio rejected');
        });

        const detached = new ArrayBuffer(4);
        structuredClone(detached, { transfer: [detached] });
        context.decodeAudioData(detached).catch(error => {
            if (error.name !== 'DataCloneError')
                throw Error('detached audio should reject with DataCloneError');
            console.log('detached audio rejected');
        });

        const oversized = new ArrayBuffer(8 * 1024 * 1024 + 1);
        context.decodeAudioData(oversized).catch(error => {
            if (error.name !== 'NotSupportedError')
                throw Error('oversized audio should reject before copying');
            console.log('oversized audio rejected');
        });
        if (oversized.byteLength !== 0)
            throw Error('oversized encoded input was not detached');
    </script>"#,
        &[
            "log: unsupported audio rejected",
            "log: detached audio rejected",
            "log: oversized audio rejected",
        ],
    );
    assert_eq!(console.len(), 3, "unexpected decode logs: {console:?}");
}

#[test]
fn decode_audio_data_accepts_complete_native_flac_in_audio_context() {
    // The fixture is a self-authored, one-second 440 Hz mono tone. Exercise the
    // JavaScript/worker boundary as well as the codec, not just the Rust parser.
    let encoded = include_str!("../../../../tests/fixtures/media/test-1s-audio.flac.base64")
        .lines()
        .collect::<String>();
    let html = r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 48000);
        const binary = atob('__FLAC_FIXTURE__');
        const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
        const input = bytes.buffer;
        context.decodeAudioData(input).then(buffer => {
            const channel = buffer.getChannelData(0);
            if (buffer.sampleRate !== 48000 || buffer.numberOfChannels !== 1 ||
                buffer.length !== 48000 ||
                !channel.some(sample => sample > 0.05) ||
                !channel.some(sample => sample < -0.05))
                throw Error('FLAC decoded AudioBuffer was not real resampled PCM');
            console.log('FLAC decodeAudioData passed');
        });
        if (input.byteLength !== 0) throw Error('FLAC ArrayBuffer was not detached');
    </script>"#
        .replace("__FLAC_FIXTURE__", &encoded);
    let console = execute_decode_html_until_logged(&html, &["log: FLAC decodeAudioData passed"]);
    assert_eq!(console, ["log: FLAC decodeAudioData passed"]);
}

#[test]
fn decode_audio_data_accepts_ogg_mp3_and_m4a_in_audio_context() {
    // Synthetic fixtures traverse the page API and native worker. A byte
    // sniffer or metadata-only decoder cannot satisfy these PCM assertions.
    let cases = [
        (
            "Ogg/Vorbis",
            include_str!("../../../../tests/fixtures/media/test-2s-audio.ogg.base64"),
            92_000,
            98_000,
        ),
        (
            "MP3",
            include_str!("../../../../tests/fixtures/media/test-0.4s-tone.mp3.base64"),
            17_000,
            22_000,
        ),
        (
            "AAC-in-M4A",
            include_str!("../../../../tests/fixtures/media/test-0.4s-tone.m4a.base64"),
            17_000,
            22_000,
        ),
    ];
    for (label, fixture, min_frames, max_frames) in cases {
        let encoded = fixture.lines().collect::<String>();
        let html = r#"<body><script>
            const context = new OfflineAudioContext(1, 128, 48000);
            const binary = atob('__FIXTURE__');
            const input = Uint8Array.from(binary,
                character => character.charCodeAt(0)).buffer;
            context.decodeAudioData(input).then(buffer => {
                const channel = buffer.getChannelData(0);
                if (buffer.sampleRate !== 48000 || buffer.numberOfChannels !== 1 ||
                    buffer.length < __MIN_FRAMES__ || buffer.length > __MAX_FRAMES__ ||
                    !channel.some(sample => sample > 0.02) ||
                    !channel.some(sample => sample < -0.02))
                    throw Error('audio decode did not produce resampled bipolar PCM');
                console.log('format decodeAudioData passed');
            });
            if (input.byteLength !== 0)
                throw Error('encoded ArrayBuffer was not detached');
        </script>"#
            .replace("__FIXTURE__", &encoded)
            .replace("__MIN_FRAMES__", &min_frames.to_string())
            .replace("__MAX_FRAMES__", &max_frames.to_string());
        let console =
            execute_decode_html_until_logged(&html, &["log: format decodeAudioData passed"]);
        assert_eq!(console, ["log: format decodeAudioData passed"], "{label}");
    }
}
