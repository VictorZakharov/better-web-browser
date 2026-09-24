use super::*;

#[test]
fn text_track_lists_cues_and_media_clock_are_live() {
    let (_, outcome) = execute_html(
        r#"<script>
        const video = document.createElement('video');
        document.body.appendChild(video);
        const list = video.textTracks;
        if (list !== video.textTracks || list.length !== 0) throw Error('stable initial list');
        const track = video.addTextTrack('subtitles', 'English', 'en');
        if (list.length !== 1 || list[0] !== track || list.item(0) !== track) throw Error('live list');
        if (track.kind !== 'subtitles' || track.label !== 'English' || track.language !== 'en') throw Error('identity');
        const cue = new VTTCue(1, 3, 'Hello <b>world</b> &amp; everyone');
        cue.id = 'one';
        let entered = 0, exited = 0, changed = 0;
        cue.onenter = () => entered++;
        cue.onexit = () => exited++;
        track.oncuechange = () => changed++;
        track.addCue(cue);
        if (track.cues.length !== 1 || track.cues.getCueById('one') !== cue) throw Error('cue list');
        if (cue.getCueAsHTML().textContent !== 'Hello world & everyone') throw Error('cue text parser');
        video.currentTime = 1.5;
        track.mode = 'showing';
        if (track.activeCues.length !== 1 || track.activeCues[0] !== cue || entered !== 1) throw Error('active cue');
        video.currentTime = 3;
        if (track.activeCues.length !== 0 || exited !== 1 || changed < 2) throw Error('cue exit');
        track.removeCue(cue);
        if (cue.track !== null || track.cues.length) throw Error('cue removal');
        const element = document.createElement('track');
        video.appendChild(element);
        element.kind = 'captions'; element.label = 'French'; element.srclang = 'fr';
        if (!(element instanceof HTMLTrackElement)) throw Error('track constructor');
        if (list.length !== 2) throw Error('track list length ' + list.length);
        if (element.track !== list[0] || list[1] !== track) throw Error('track order');
        if (element.track.kind !== 'captions') throw Error('track kind');
        console.log('text tracks passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: text tracks passed"]);
    assert!(outcome.media_actions.iter().any(|action| matches!(
        &action.command, ScriptMediaCommand::Caption { cues } if cues.iter().any(|cue| cue.text.contains("Hello world & everyone"))
    )));
}

#[test]
fn invalid_cues_and_track_kinds_fail_without_mutating_existing_state() {
    let (_, outcome) = execute_html(
        r#"<script>
        const video = document.createElement('video');
        const track = video.addTextTrack('captions');
        const cue = new VTTCue(0, 1, 'one');
        track.addCue(cue);
        try { video.addTextTrack('not-a-kind'); throw Error('kind accepted'); }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { track.addCue(new VTTCue(2, 1, 'backwards')); throw Error('invalid interval accepted'); }
        catch (error) { if (error.name !== 'InvalidStateError') throw error; }
        try { track.removeCue(new VTTCue(0, 1, 'missing')); throw Error('missing cue removed'); }
        catch (error) { if (error.name !== 'NotFoundError') throw error; }
        if (track.cues.length !== 1 || track.cues[0] !== cue) throw Error('state mutated');
        console.log('invalid cues passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: invalid cues passed"]);
}

#[test]
fn webvtt_track_loads_and_parses_timing_settings_and_text() {
    let (dom, mut runtime, id) = super::network::pending_runtime(
        r#"
        const video = document.createElement('video');
        const element = document.createElement('track');
        video.appendChild(element); document.body.appendChild(video);
        element.src = '/captions.vtt';
        element.addEventListener('load', () => {
            const track = element.track;
            const cue = track.cues[0];
            document.querySelector('div').textContent = [element.readyState, track.cues.length,
                cue.id, cue.startTime, cue.endTime, cue.line, cue.position, cue.align,
                cue.getCueAsHTML().textContent].join('|');
        });
        video.currentTime = 1.5;
        element.track.mode = 'showing';
    "#,
    );
    let body = b"WEBVTT sample\n\nNOTE ignored\n\nfirst\n00:01.200 --> 00:03.500 line:50% position:20% align:start\nHello <b>world</b> &amp; friends\n\nSTYLE\n::cue {color:white}\n";
    let outcome =
        runtime.complete_fetch_with_loader(id, Ok(super::network::test_response(body)), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "2|1|first|1.2|3.5|50|20|start|Hello world & friends"
    );
    assert!(outcome.media_actions.iter().any(|action| matches!(
        &action.command, ScriptMediaCommand::Caption { cues } if cues.iter().any(|cue| cue.text == "Hello world & friends")
    )));
}

#[test]
fn invalid_webvtt_resource_reports_track_error() {
    let (dom, mut runtime, id) = super::network::pending_runtime(
        r#"
        const video = document.createElement('video');
        const element = document.createElement('track');
        video.appendChild(element); document.body.appendChild(video);
        element.src = '/bad.vtt';
        element.addEventListener('error', () => {
            document.querySelector('div').textContent = 'error:' + element.readyState;
        });
        element.track.mode = 'showing';
    "#,
    );
    let outcome = runtime.complete_fetch_with_loader(
        id,
        Ok(super::network::test_response(
            b"NOT VTT\n\n00:00.000 --> 00:01.000\ntext",
        )),
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "error:3"
    );
}
