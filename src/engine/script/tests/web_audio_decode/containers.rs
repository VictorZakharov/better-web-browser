use super::*;

fn cases() -> [(&'static str, &'static str, usize, usize); 3] {
    [
        (
            "ADTS AAC-LC",
            include_str!("../../../../../tests/fixtures/media/test-0.4s-tone.aac.base64"),
            18_000,
            24_000,
        ),
        (
            "WebM/Vorbis",
            include_str!("../../../../../tests/fixtures/media/test-0.4s-tone.webm.base64"),
            17_000,
            22_000,
        ),
        (
            "Ogg/FLAC",
            include_str!("../../../../../tests/fixtures/media/test-0.4s-tone.oga.base64"),
            19_200,
            19_200,
        ),
    ]
}

#[test]
fn decode_audio_data_new_containers_preserve_pcm_duration_detachment_and_callbacks() {
    for (label, fixture, min_frames, max_frames) in cases() {
        let encoded = fixture.lines().collect::<String>();
        let html = r#"<body><script>
            const context = new OfflineAudioContext(1, 128, 48000);
            const input = Uint8Array.from(atob('__FIXTURE__'),
                character => character.charCodeAt(0)).buffer;
            const events = [], encodedBytes = input.byteLength;
            const promise = context.decodeAudioData(input, buffer => {
                events.push('callback');
                if (buffer.sampleRate !== 48000 || buffer.numberOfChannels !== 1 ||
                    buffer.length < __MIN__ || buffer.length > __MAX__ ||
                    Math.abs(buffer.duration - buffer.length / 48000) > 1e-9)
                    throw Error('incorrect decoded metadata or duration');
            });
            if (encodedBytes === 0 || input.byteLength !== 0)
                throw Error('encoded input was not detached synchronously');
            events.push('returned');
            promise.then(buffer => {
                events.push('then');
                const channel = buffer.getChannelData(0);
                if (!channel.every(Number.isFinite) ||
                    !channel.some(sample => sample > 0.02) ||
                    !channel.some(sample => sample < -0.02) ||
                    events.join(',') !== 'returned,callback,then')
                    throw Error('missing decoded PCM or incorrect promise/callback order');
                console.log('new container decode passed');
            });
        </script>"#
            .replace("__FIXTURE__", &encoded)
            .replace("__MIN__", &min_frames.to_string())
            .replace("__MAX__", &max_frames.to_string());
        let console =
            execute_decode_html_until_logged(&html, &["log: new container decode passed"]);
        assert_eq!(console, ["log: new container decode passed"], "{label}");
    }
}

#[test]
fn decode_audio_data_new_containers_reject_truncation_through_shared_encoding_error() {
    for (label, fixture, _, _) in cases() {
        let encoded = fixture.lines().collect::<String>();
        let html = r#"<body><script>
            const context = new OfflineAudioContext(1, 128, 48000);
            const bytes = Uint8Array.from(atob('__FIXTURE__'),
                character => character.charCodeAt(0));
            const input = bytes.slice(0, Math.floor(bytes.length / 2)).buffer;
            let callbackError;
            context.decodeAudioData(input, () => { throw Error('partial decode accepted'); }, error => {
                callbackError = error;
            }).then(() => { throw Error('partial decode promise fulfilled'); }, error => {
                if (error.name !== 'EncodingError' || error !== callbackError)
                    throw Error('decode rejection identity or type changed');
                console.log('new container truncation rejected');
            });
            if (input.byteLength !== 0) throw Error('failed decode input was not detached');
        </script>"#
            .replace("__FIXTURE__", &encoded);
        let console =
            execute_decode_html_until_logged(&html, &["log: new container truncation rejected"]);
        assert_eq!(
            console,
            ["log: new container truncation rejected"],
            "{label}"
        );
    }
}
