use super::*;

#[test]
fn abort_discards_partial_input_even_between_appends_and_preserves_buffered_frames() {
    let (dom, mut runtime, initial) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        const box = (name, value) => new Uint8Array([0,0,0,9,...[...name].map(c => c.charCodeAt(0)),value]);
        const join = (...parts) => new Uint8Array(parts.flatMap(p => [...p]));
        const init = join(box('ftyp',0),box('moov',1));
        const media = n => join(box('moof',0),box('mdat',n));
        source.addEventListener('sourceopen', () => {
            const video = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            const audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            let step = 0;
            video.addEventListener('updateend', () => {
                if (++step === 1) video.appendBuffer(join(box('moof',0),[0,0,0,64,109,100,97,116,1,2]));
                else if (step === 2) {
                    video.appendWindowStart = 10; video.appendWindowEnd = 20;
                    video.abort();
                    document.querySelector('output').textContent =
                        [video.appendWindowStart,video.appendWindowEnd,video.buffered.end(0)].join(':');
                    video.appendBuffer(media(99));
                }
            });
            video.appendBuffer(join(init,media(1)));
            audio.appendBuffer(join(init,media(2)));
        });
        movie.src = URL.createObjectURL(source);
    </script>"#,
    );
    assert!(initial.errors.is_empty());
    let result = runtime
        .dispatch_media_and_tasks(UserInputEvent::Media {
            target: dom.elements_named("video").next().unwrap(),
            request_id: 0,
            disposition: "loaded",
            current_time: 0.0,
            duration: 1.0,
            width: 320,
            height: 240,
            buffered: Some([[0.0, 1.0]; 2]),
        })
        .outcome;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let bytes = result
        .media_actions
        .iter()
        .find_map(|action| match &action.command {
            ScriptMediaCommand::AppendAdaptive { video_bytes, .. } => Some(video_bytes),
            _ => None,
        })
        .expect("fresh segment must not be trapped behind the canceled partial mdat");
    assert_eq!(bytes.len(), 36);
    assert_eq!(*bytes.last().unwrap(), 99);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0:Infinity:1"
    );
}

#[test]
fn abort_during_updatestart_cancels_the_old_append_and_removal_cannot_be_aborted() {
    let (dom, _, result) = execute_media_source(
        r#"<video id="movie"></video><output></output><script>
        const source = new MediaSource();
        source.addEventListener('sourceopen', () => {
            const video = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
            source.duration = 10;
            let events = [], updates = 0;
            video.addEventListener('updatestart', () => { events.push('start'); video.abort(); }, {once:true});
            video.addEventListener('abort', () => events.push('abort'));
            video.addEventListener('update', () => events.push('update'));
            video.addEventListener('updateend', () => {
                events.push('end');
                if (++updates === 1) {
                    video.remove(0, 1);
                    try { video.abort(); } catch (e) { events.push(e.name); }
                } else document.querySelector('output').textContent = events.join(',');
            });
            // Invalid bytes must never be parsed after the updatestart handler aborts.
            video.appendBuffer(new Uint8Array([0,0,0,1,0,0,0,0,255,255,255,255,255,255,255,255]));
        });
        movie.src = URL.createObjectURL(source);
    </script>"#,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.media_actions.is_empty());
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "start,abort,end,InvalidStateError,update,end"
    );
}
