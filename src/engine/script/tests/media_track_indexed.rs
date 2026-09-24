use super::media_source_segments::{MediaTaskTestRuntime, execute_media_source};
use super::*;

#[test]
fn decoded_track_lists_expose_live_read_only_indexed_properties() {
    let (dom, mut runtime, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const audio = movie.audioTracks, video = movie.videoTracks;
        const checks = [audio[0] === undefined, video[0] === undefined,
            !('0' in audio), !('0' in video), audio.item(0) === null];
        movie.addEventListener('loadedmetadata', () => {
            for (const list of [audio, video]) {
                const descriptor = Object.getOwnPropertyDescriptor(list, '0');
                checks.push('0' in list, list[0] === list.item(0),
                    list[1] === undefined, !('1' in list),
                    descriptor.value === list[0] && descriptor.enumerable &&
                        descriptor.configurable && !descriptor.writable,
                    Object.keys(list).includes('0'),
                    !Reflect.set(list, '0', null),
                    !Reflect.defineProperty(list, '0', {value: null}),
                    !Reflect.deleteProperty(list, '0'));
            }
            movie.load();
            checks.push(audio.length === 0, video.length === 0,
                audio[0] === undefined, video[0] === undefined,
                !('0' in audio), !('0' in video));
            document.querySelector('output').textContent = checks.join(',');
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
        vec!["true"; 29].join(",")
    );
}

#[test]
fn script_text_track_list_shares_the_same_indexed_property_contract() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video'), list = movie.textTracks;
        const absent = list[0] === undefined && !('0' in list);
        const track = movie.addTextTrack('captions', 'English', 'en');
        const descriptor = Object.getOwnPropertyDescriptor(list, '0');
        const checks = [absent, list[0] === track, '0' in list,
            list.item(0) === track, list.getTrackById('') === track,
            Object.keys(list).includes('0'), descriptor.value === track,
            !descriptor.writable, !Reflect.deleteProperty(list, '0'),
            !Reflect.set(list, '0', null)];
        document.querySelector('output').textContent = checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        ["true"; 10].join(",")
    );
}

#[test]
fn cue_lists_update_indexed_properties_as_cues_are_added_and_removed() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
        const movie = document.querySelector('video');
        const track = movie.addTextTrack('metadata');
        track.mode = 'hidden';
        const cues = track.cues, active = track.activeCues;
        const checks = [cues.length === 0, cues[0] === undefined,
            !('0' in cues), active.length === 0, active[0] === undefined];
        const first = new VTTCue(0, 2, 'first');
        first.id = 'first';
        track.addCue(first);
        const descriptor = Object.getOwnPropertyDescriptor(cues, '0');
        checks.push(cues.length === 1, cues[0] === first, '0' in cues,
            cues.getCueById('first') === first, descriptor.value === first,
            descriptor.enumerable && !descriptor.writable,
            Object.keys(cues).includes('0'), !Reflect.deleteProperty(cues, '0'));
        track.removeCue(first);
        checks.push(cues.length === 0, cues[0] === undefined,
            !('0' in cues), cues.getCueById('first') === null);
        document.querySelector('output').textContent = checks.join(',');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        vec!["true"; 17].join(",")
    );
}
