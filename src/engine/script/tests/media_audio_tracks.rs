use super::media_source_segments::{MediaTaskTestRuntime, execute_media_source};
use super::*;

#[test]
fn decoded_audio_track_is_live_and_controls_the_mixer() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const video = document.querySelector('video'), output = document.querySelector('output');
        const tracks = video.audioTracks;
        const seen = [];
        seen.push(tracks === video.audioTracks && tracks.length === 0 && tracks[0] == null);
        seen.push(tracks instanceof AudioTrackList && 'onaddtrack' in tracks);
        seen.push(typeof TrackEvent === 'function');
        tracks.addEventListener('addtrack', event => {
            const track = event.track;
            seen.push(event instanceof TrackEvent && event.isTrusted);
            seen.push(track instanceof AudioTrack && track === tracks[0]);
            seen.push(tracks.length === 1 && tracks.getTrackById(track.id) === track);
            seen.push(track.kind === 'main' && track.enabled);
            track.enabled = false;
            seen.push(!track.enabled && !video.muted && video.volume === 1);
            output.textContent = seen.join(',');
        });
        tracks.onchange = () => output.textContent += '|change';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: dom.elements_named("video").next().unwrap(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 8.0,
        width: 320,
        height: 240,
        buffered: None,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true,true,true,true,true|change"
    );
    assert!(result.outcome.media_actions.iter().any(|action| matches!(
        action.command,
        ScriptMediaCommand::Configure {
            volume_millis: 0,
            ..
        }
    )));
}

#[test]
fn track_event_validates_its_track_and_audio_list_is_not_constructible() {
    let (dom, outcome) = execute_html(
        r#"<output></output><script>
        const failed = fn => { try { fn(); return false; } catch (e) { return e instanceof TypeError; } };
        const event = new TrackEvent('addtrack');
        document.querySelector('output').textContent = [
            event.track === null,
            failed(() => new TrackEvent('addtrack', { track: {} })),
            failed(() => new AudioTrackList()),
            failed(() => new AudioTrack())
        ].join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true,true,true,true"
    );
}

#[test]
fn script_created_text_track_queues_a_track_event_with_its_new_track() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const list = movie.textTracks;
        const order = [];
        list.onaddtrack = event => document.querySelector('output').textContent = order.join(':') + ':' + [
            event instanceof TrackEvent, event.track === list[0], event.track.kind,
            event.isTrusted
        ].join(':');
        const track = movie.addTextTrack('captions');
        order.push('sync');
        if (list[0] !== track || list.length !== 1)
            throw new Error('A script-created track was not added synchronously');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "sync:true:true:captions:true"
    );
}

#[test]
fn replacing_media_removes_tracks_and_cancels_stale_addtrack_tasks() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const audio = movie.audioTracks, video = movie.videoTracks;
        const order = [];
        audio.onaddtrack = event => {
            order.push('audio-add:' + (event.track === audio[0]));
            movie.load();
            order.push('reset:' + audio.length + ':' + video.length);
        };
        video.onaddtrack = () => order.push('stale-video-add');
        audio.onremovetrack = event => {
            order.push('audio-remove:' + (event.track instanceof AudioTrack));
            document.querySelector('output').textContent = order.join('|');
        };
        video.onremovetrack = event => {
            order.push('video-remove:' + (event.track instanceof VideoTrack));
            document.querySelector('output').textContent = order.join('|');
        };
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: dom.elements_named("video").next().unwrap(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 8.0,
        width: 320,
        height: 240,
        buffered: None,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "audio-add:true|reset:0:0|audio-remove:true|video-remove:true"
    );
    assert!(
        result
            .outcome
            .media_actions
            .iter()
            .any(|action| matches!(action.command, ScriptMediaCommand::Reset))
    );
}
