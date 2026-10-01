use super::*;

#[test]
fn opus_script_queries_admit_actual_recording_and_webm_but_not_unimplemented_streaming() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
        const type = 'audio/ogg;codecs="opus"';
        if (new Audio().canPlayType(type) !== 'probably') throw Error('missing actual decoder');
        if (new Audio().canPlayType('audio/webm;codecs=opus') !== 'probably') throw Error('WebM trim missing');
        if (MediaSource.isTypeSupported(type)) throw Error('unimplemented SourceBuffer');
        if (!MediaRecorder.isTypeSupported(type)) throw Error('missing actual Ogg recorder');
        if (!MediaRecorder.isTypeSupported('audio/webm;codecs=opus')) throw Error('WebM recording missing');
        const audio = {contentType: type, channels: '2', bitrate: 64000, samplerate: 48000};
        Promise.all([
            navigator.mediaCapabilities.decodingInfo({type:'file', audio}),
            navigator.mediaCapabilities.decodingInfo({type:'file', audio:{...audio, contentType:'audio/webm;codecs=opus'}}),
            navigator.mediaCapabilities.decodingInfo({type:'file', audio:{...audio, samplerate:44100}}),
            navigator.mediaCapabilities.decodingInfo({type:'file', audio:{...audio, channels:'3'}}),
            navigator.mediaCapabilities.decodingInfo({type:'media-source', audio})
        ]).then(([supported, webm, ...unsupported]) => {
            if ([supported, webm].some(info => !info.supported || info.smooth || info.powerEfficient || info.keySystemAccess !== null))
                throw Error('truthful supported decoder');
            if (unsupported.some(report => report.supported || report.smooth || report.powerEfficient))
                throw Error('overclaimed bounded scope');
            document.querySelector('output').textContent='passed';
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
