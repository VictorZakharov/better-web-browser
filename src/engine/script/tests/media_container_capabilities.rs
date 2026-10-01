use super::*;

#[test]
fn new_audio_container_queries_distinguish_playback_streaming_and_recording() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const audio = new Audio();
            const formats = ['audio/aac;codecs="mp4a.40.2"', 'audio/webm;codecs="vorbis"',
                'audio/ogg;codecs="flac"'];
            for (const type of formats) {
                if (audio.canPlayType(type) !== 'probably') throw Error('complete-file decoder');
                if (MediaSource.isTypeSupported(type)) throw Error('unimplemented SourceBuffer');
                if (MediaRecorder.isTypeSupported(type)) throw Error('unimplemented encoder');
            }
            const capabilityFormats = ['audio/aac', ...formats.slice(1)];
            Promise.all(capabilityFormats.map(contentType => navigator.mediaCapabilities.decodingInfo({
                type: 'file', audio: {contentType, channels: '2', bitrate: 128000, samplerate: 44100}
            }))).then(reports => {
                if (!reports.every(report => report.supported && !report.powerEfficient &&
                    report.keySystemAccess === null)) throw Error('file decoder capability');
                return Promise.all(capabilityFormats.map(contentType => navigator.mediaCapabilities.decodingInfo({
                    type: 'media-source',
                    audio: {contentType, channels: '2', bitrate: 128000, samplerate: 44100}
                })));
            }).then(reports => {
                if (reports.some(report => report.supported)) throw Error('streaming overclaim');
                document.querySelector('output').textContent = 'passed';
            });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn media_queries_observe_quoted_parameters_and_first_valid_codec() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
            const audio = new Audio();
            const checks = [
                ['audio/webm;note="ignored;codecs=opus";codecs=vorbis', 'probably'],
                ['audio/webm;codecs=vorbis;codecs=opus', 'probably'],
                ['audio/webm;codecs=opus;codecs=vorbis', 'probably'],
                ['audio/webm;codecs=;codecs=vorbis', 'probably'],
                ['audio/webm;codecs="";codecs=vorbis', ''],
                ['audio/webm;codecs="vor\\bis"', 'probably'],
                ['audio/ogg;codecs=flac,opus', ''],
                ['video/webm;codecs=vorbis', ''],
                ['audio/webm;codecs=vorbis,vorbis', ''],
                ['audio/webm;codecs=vorbis,', '']
            ];
            for (const [type, expected] of checks) {
                if (audio.canPlayType(type) !== expected) throw Error(type);
            }
            if (!MediaSource.isTypeSupported('audio/mp4;note=";codecs=unknown";codecs=mp4a.40.2'))
                throw Error('MP4 MIME parsing');
            document.querySelector('output').textContent = 'passed';
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn sourcebuffer_configuration_uses_parsed_codecs_not_unrelated_parameters() {
    let (dom, _runtime, outcome) = super::media_source_segments::execute_media_source(
        r#"<video></video><output>pending</output><script>
            const source = new MediaSource();
            source.addEventListener('sourceopen', () => {
                // Neither quoted note should convert a single-track buffer to muxed.
                source.addSourceBuffer('audio/mp4;note="avc1.42e01e";codecs=mp4a.40.2');
                source.addSourceBuffer('video/mp4;note="mp4a.40.2";codecs=avc1.42e01e');
                if (source.sourceBuffers.length !== 2) throw Error('separate track admission');
                for (const buffer of [...source.sourceBuffers]) source.removeSourceBuffer(buffer);
                source.addSourceBuffer('video/mp4;codecs=mp4a.40.2;codecs=avc1.42e01e');
                source.addSourceBuffer('video/mp4;codecs=avc1.42e01e;codecs=mp4a.40.2');
                if (source.sourceBuffers.length !== 2) throw Error('first codec parameter');
                document.querySelector('output').textContent = 'passed';
            });
            document.querySelector('video').src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
