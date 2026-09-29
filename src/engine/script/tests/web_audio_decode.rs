use super::*;

#[test]
fn decode_audio_data_detaches_synchronously_and_returns_resampled_pcm() {
    let (_, outcome) = execute_html(
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
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: WAV decode and detach passed"]);
}

#[test]
fn decode_audio_data_rejects_unsupported_bytes_and_detached_input() {
    let (_, outcome) = execute_html(
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
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome
            .console
            .contains(&"log: unsupported audio rejected".to_owned())
    );
    assert!(
        outcome
            .console
            .contains(&"log: detached audio rejected".to_owned())
    );
    assert!(
        outcome
            .console
            .contains(&"log: oversized audio rejected".to_owned())
    );
}
