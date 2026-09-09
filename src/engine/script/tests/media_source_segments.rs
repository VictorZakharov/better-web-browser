use super::*;
#[path = "media_source_abort.rs"]
mod abort;
#[path = "media_source_reconfiguration.rs"]
mod reconfiguration;
#[path = "media_source_seeking.rs"]
mod seeking;

#[test]
fn media_source_declared_duration_survives_partial_decode_and_append() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<body><video id="movie"></video><output></output><script>
            const source = new MediaSource();
            const movie = document.getElementById('movie');
            source.addEventListener('sourceopen', () => {
                source.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
                source.duration = 220;
            });
            const record = () => document.querySelector('output').textContent =
                [source.duration, movie.duration, movie.buffered.end(0), movie.seekable.end(0)].join(':');
            movie.addEventListener('loadedmetadata', record);
            movie.addEventListener('progress', record);
            movie.src = URL.createObjectURL(source);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    for (disposition, duration, expected) in [
        ("loaded", 10.0, "220:220:10:220"),
        ("appended", 40.0, "220:220:40:220"),
        ("appended", 230.0, "230:230:230:230"),
    ] {
        let result = runtime.dispatch_user_input(UserInputEvent::Media {
            buffered: None,
            target: dom.elements_named("video").next().unwrap(),
            request_id: 0,
            disposition,
            current_time: 0.0,
            duration,
            width: 1280,
            height: 720,
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            expected
        );
    }
}

#[test]
fn media_source_duration_assignment_validates_state_and_numbers() {
    let (dom, _, outcome) = execute_media_source(
        r#"<body><video></video><output></output><script>
            const source = new MediaSource();
            const results = [];
            const attempt = value => {
                try { source.duration = value; results.push(String(source.duration)); }
                catch (error) { results.push(error.name); }
            };
            attempt(NaN); attempt(-1); attempt(5);
            source.addEventListener('sourceopen', () => {
                attempt(Infinity); attempt(220); attempt(0);
                document.querySelector('output').textContent = results.join(':');
            });
            document.querySelector('video').src = URL.createObjectURL(source);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "TypeError:TypeError:InvalidStateError:Infinity:220:0"
    );
}

pub(super) fn execute_media_source(
    html: &str,
) -> (super::super::super::dom::Dom, ScriptRuntime, ScriptOutcome) {
    let dom = dom::parse_with_scripting(html, true);
    let script = dom.elements_named("script").next().unwrap();
    let input = ScriptInput {
        source_url: "https://example.com/#media-source-segments".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    };
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[input]);
    (dom, runtime, outcome)
}

#[test]
fn open_media_source_waits_at_buffer_end_and_does_not_resume_a_user_pause() {
    for pause_while_waiting in [false, true] {
        let script = format!(
            r#"<body><video id="movie"></video><output></output><span id="ready"></span><script>
            const source = new MediaSource();
            const movie = document.getElementById('movie');
            let canPlayEvents = 0;
            movie.addEventListener('canplay', () => {{
                document.getElementById('ready').textContent =
                    [++canPlayEvents, movie.readyState].join(':');
            }});
            source.addEventListener('sourceopen', () => {{
                source.addSourceBuffer('video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
                source.duration = 220;
            }});
            movie.addEventListener('waiting', () => {{
                document.querySelector('output').textContent =
                    [movie.ended, movie.currentTime, movie.duration].join(':');
                if ({pause_while_waiting}) movie.pause();
            }});
            movie.src = URL.createObjectURL(source);
        </script></body>"#
        );
        let (dom, mut runtime, outcome) = execute_media_source(&script);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        for (disposition, current_time, duration) in [
            ("loaded", 0.0, 10.0),
            ("playing", 0.0, 10.0),
            ("ended", 10.0, 10.0),
            ("appended", 10.0, 20.0),
            ("appended", 10.0, 30.0),
        ] {
            let result = runtime.dispatch_user_input(UserInputEvent::Media {
                buffered: None,
                target: dom.elements_named("video").next().unwrap(),
                request_id: 0,
                disposition,
                current_time,
                duration,
                width: 1280,
                height: 720,
            });
            assert!(
                result.outcome.errors.is_empty(),
                "{:?}",
                result.outcome.errors
            );
            if disposition == "appended" {
                let resumes = result.outcome.media_actions.iter().any(|action| {
                    matches!(
                        action.command,
                        ScriptMediaCommand::SetPlayback { playing: true, .. }
                    )
                });
                assert_eq!(resumes, !pause_while_waiting && duration == 20.0);
            }
        }
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            "false:10:220"
        );
        assert_eq!(
            dom.elements_named("span").next().unwrap().text_content(),
            "2:4"
        );
    }
}

#[test]
fn media_source_does_not_commit_a_moof_before_its_mdat_is_complete() {
    let (dom, _, outcome) = execute_media_source(
        r#"<body><video id="movie"></video><script>
            const mediaSource = new MediaSource();
            const movie = document.getElementById('movie');
            const box = (kind, payload = []) => new Uint8Array([
                0, 0, 0, payload.length + 8,
                ...Array.from(kind, character => character.charCodeAt(0)),
                ...payload
            ]);
            const concat = parts => {
                const bytes = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
                let offset = 0;
                for (const part of parts) { bytes.set(part, offset); offset += part.length; }
                return bytes;
            };
            mediaSource.addEventListener('sourceopen', () => {
                const buffer = mediaSource.addSourceBuffer(
                    'video/mp4; codecs="avc1.4d401e, mp4a.40.2"');
                const completeSegment = concat([
                    box('ftyp'), box('moov'), box('moof'), box('mdat', [1, 2, 3, 4])
                ]);
                const nextMoof = box('moof');
                const partialMdat = box('mdat', [5, 6, 7, 8]).slice(0, 9);
                buffer.appendBuffer(concat([completeSegment, nextMoof, partialMdat]));
                movie.play();
            }, { once: true });
            movie.src = URL.createObjectURL(mediaSource);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let committed = outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::Commit { bytes, .. } => Some(bytes),
            _ => None,
        })
        .expect("the first complete media segment was not committed");

    let expected = [
        0, 0, 0, 8, b'f', b't', b'y', b'p', 0, 0, 0, 8, b'm', b'o', b'o', b'v', 0, 0, 0, 8, b'm',
        b'o', b'o', b'f', 0, 0, 0, 12, b'm', b'd', b'a', b't', 1, 2, 3, 4,
    ];
    assert_eq!(committed, &expected);
    assert_eq!(dom.elements_named("video").count(), 1);
}

#[test]
fn adaptive_media_source_sends_later_segments_as_bounded_appends() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<body><video id="movie"></video><script>
            const mediaSource = new MediaSource();
            const movie = document.getElementById('movie');
            const box = (kind, payload = []) => new Uint8Array([
                0, 0, 0, payload.length + 8,
                ...Array.from(kind, character => character.charCodeAt(0)),
                ...payload
            ]);
            const concat = parts => {
                const bytes = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
                let offset = 0;
                for (const part of parts) { bytes.set(part, offset); offset += part.length; }
                return bytes;
            };
            const initial = concat([
                box('ftyp'), box('moov'), box('moof'), box('mdat', [1])
            ]);
            const later = concat([box('moof'), box('mdat', [2])]);
            mediaSource.addEventListener('sourceopen', () => {
                const video = mediaSource.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                const audio = mediaSource.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
                video.appendBuffer(initial);
                audio.appendBuffer(initial);
                movie.play();
                let completed = 0;
                for (const buffer of [video, audio]) buffer.addEventListener('updateend', () => {
                    if (++completed === 2) {
                        video.appendBuffer(later);
                        audio.appendBuffer(later);
                    }
                }, { once: true });
            }, { once: true });
            movie.src = URL.createObjectURL(mediaSource);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let initial = outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::CommitAdaptive {
                video_bytes,
                audio_bytes,
                ..
            } => Some((video_bytes, audio_bytes)),
            _ => None,
        })
        .expect("the first adaptive segment was not committed");
    assert_eq!(initial.0.last(), Some(&1));
    assert_eq!(initial.1.last(), Some(&1));

    let video = dom.elements_named("video").next().unwrap();
    let loaded = runtime.dispatch_user_input(UserInputEvent::Media {
        buffered: None,
        target: video,
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 10.0,
        width: 1280,
        height: 720,
    });
    assert!(
        loaded.outcome.errors.is_empty(),
        "{:?}",
        loaded.outcome.errors
    );
    let appended = loaded
        .outcome
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::AppendAdaptive {
                video_bytes,
                audio_bytes,
            } => Some((video_bytes, audio_bytes)),
            _ => None,
        })
        .expect("the later adaptive segment was not sent as an append");
    let expected = [
        0, 0, 0, 8, b'f', b't', b'y', b'p', 0, 0, 0, 8, b'm', b'o', b'o', b'v', 0, 0, 0, 8, b'm',
        b'o', b'o', b'f', 0, 0, 0, 9, b'm', b'd', b'a', b't', 2,
    ];
    assert_eq!(appended.0, &expected);
    assert_eq!(appended.1, &expected);
}
