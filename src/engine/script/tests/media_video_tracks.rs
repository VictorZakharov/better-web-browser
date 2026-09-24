use super::media_source_segments::{MediaTaskTestRuntime, execute_media_source};
use super::*;

#[test]
fn decoded_video_track_selection_emits_a_renderer_action_and_change_event() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const list = movie.videoTracks;
        const seen = [list === movie.videoTracks, list.length === 0, list.selectedIndex === -1];
        list.onaddtrack = event => {
            const track = event.track;
            seen.push(event instanceof TrackEvent && event.isTrusted);
            seen.push(track instanceof VideoTrack && track === list[0]);
            seen.push(track.selected && list.selectedIndex === 0);
            seen.push(list.getTrackById(track.id) === track);
            track.selected = false;
            seen.push(!track.selected && list.selectedIndex === -1);
            document.querySelector('output').textContent = seen.join(',');
        };
        list.onchange = event => document.querySelector('output').textContent +=
            '|change:' + event.isTrusted;
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
        "true,true,true,true,true,true,true,true|change:true"
    );
    assert!(result.outcome.media_actions.iter().any(|action| matches!(
        action.command,
        ScriptMediaCommand::SelectVideo { selected: false }
    )));
}

#[test]
fn missing_video_frames_do_not_create_a_video_track() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        movie.addEventListener('loadedmetadata', () => {
            document.querySelector('output').textContent =
                movie.audioTracks.length + ':' + movie.videoTracks.length;
        });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let result = runtime.dispatch_media_and_tasks(UserInputEvent::Media {
        target: dom.elements_named("video").next().unwrap(),
        request_id: 0,
        disposition: "loaded",
        current_time: 0.0,
        duration: 8.0,
        width: 0,
        height: 0,
        buffered: None,
    });
    assert!(
        result.outcome.errors.is_empty(),
        "{:?}",
        result.outcome.errors
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1:0"
    );
}

#[test]
fn video_selection_is_idempotent_and_removed_track_cannot_select_a_new_resource() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const tracks = movie.videoTracks;
        const events = [];
        tracks.onaddtrack = event => {
            const track = event.track;
            track.selected = false;
            track.selected = false;
            track.selected = true;
            movie.load();
            track.selected = false;
            document.querySelector('output').textContent = [
                tracks.length === 0, tracks.selectedIndex === -1,
                tracks[0] === undefined, !track.selected
            ].join(':');
        };
        tracks.onchange = () => events.push('change');
        tracks.onremovetrack = event => {
            document.querySelector('output').textContent += ':' +
                (event.track instanceof VideoTrack) + ':' + events.length;
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
        "true:true:true:true:true:2"
    );
    let selected: Vec<_> = result
        .outcome
        .media_actions
        .iter()
        .filter_map(|action| {
            if let ScriptMediaCommand::SelectVideo { selected } = action.command {
                Some(selected)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(selected, [false, true]);
}
