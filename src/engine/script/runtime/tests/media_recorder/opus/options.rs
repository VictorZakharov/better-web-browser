use super::*;

#[test]
fn recorder_dictionary_getters_are_read_and_converted_once_in_webidl_order() {
    let (dom, mut runtime, _) = start("{mimeType: 'audio/ogg'}", "");
    run(
        &dom,
        &mut runtime,
        r#"
        const reads = [];
        const options = {};
        const values = {
            audioBitrateMode: 'constant', audioBitsPerSecond: 24000.9,
            bitsPerSecond: 64000.9, mimeType: 'audio/ogg', videoBitsPerSecond: 12000,
            videoKeyFrameIntervalCount: undefined, videoKeyFrameIntervalDuration: undefined
        };
        for (const key of Object.keys(values).reverse()) {
            Object.defineProperty(options, key, {get() {
                reads.push('get:' + key);
                const value = values[key];
                if (value === undefined) return value;
                return typeof value === 'string' ? {toString() {
                    reads.push('string:' + key); return value;
                }} : {valueOf() {reads.push('number:' + key); return value;}};
            }});
        }
        const converted = new MediaRecorder(stream, options);
        document.body.setAttribute('data-getters', reads.join(','));
        document.body.setAttribute('data-rate', String(converted.audioBitsPerSecond));
        document.body.setAttribute('data-mode', converted.audioBitrateMode);
        document.body.setAttribute('data-video-rate', String(converted.videoBitsPerSecond));
    "#,
    );
    assert_eq!(
        attribute(&dom, "data-getters"),
        concat!(
            "get:audioBitrateMode,string:audioBitrateMode,",
            "get:audioBitsPerSecond,number:audioBitsPerSecond,",
            "get:bitsPerSecond,number:bitsPerSecond,",
            "get:mimeType,string:mimeType,",
            "get:videoBitsPerSecond,number:videoBitsPerSecond,",
            "get:videoKeyFrameIntervalCount,get:videoKeyFrameIntervalDuration"
        )
    );
    assert_eq!(attribute(&dom, "data-rate"), "64000");
    assert_eq!(attribute(&dom, "data-mode"), "constant");
    assert_eq!(attribute(&dom, "data-video-rate"), "0");
}

#[test]
fn invalid_mode_and_symbol_or_bigint_options_throw_before_any_native_recording() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "");
    run(
        &dom,
        &mut runtime,
        r#"
        const names = [];
        for (const options of [
            {mimeType: 'audio/ogg', audioBitrateMode: 'unsupported'},
            {mimeType: Object(Symbol('boxed'))},
            {mimeType: 'audio/ogg', bitsPerSecond: 1n},
            {mimeType: 'audio/ogg', audioBitsPerSecond: Symbol('bits')}
        ]) {
            try {new MediaRecorder(stream, options); names.push('accepted');}
            catch (error) {names.push(error.name);}
        }
        document.body.setAttribute('data-option-errors', names.join(','));
        const fallback = new MediaRecorder(stream,
            {mimeType: 'audio/flac', audioBitrateMode: 'constant'});
        document.body.setAttribute('data-fallback-mode', fallback.audioBitrateMode);
        recorder.start();
    "#,
    );
    assert_eq!(
        attribute(&dom, "data-option-errors"),
        "TypeError,TypeError,TypeError,TypeError"
    );
    assert_eq!(attribute(&dom, "data-fallback-mode"), "variable");
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(&dom, &mut runtime, "recorder.stop();");
    decoded(&dom, 1, 960);
}

#[test]
fn explicit_video_bitrate_is_reflected_unless_aggregate_allocates_only_audio() {
    let (dom, mut runtime, _) = start("{mimeType: 'audio/ogg'}", "");
    run(
        &dom,
        &mut runtime,
        r#"
        const explicit = new MediaRecorder(stream,
            {mimeType: 'audio/ogg', videoBitsPerSecond: 12345.9});
        const aggregate = new MediaRecorder(stream,
            {mimeType: 'audio/ogg', videoBitsPerSecond: 12345.9, bitsPerSecond: 64000});
        document.body.setAttribute('data-explicit-video', String(explicit.videoBitsPerSecond));
        document.body.setAttribute('data-aggregate-video', String(aggregate.videoBitsPerSecond));
        document.body.setAttribute('data-aggregate-audio', String(aggregate.audioBitsPerSecond));
    "#,
    );
    assert_eq!(attribute(&dom, "data-explicit-video"), "12345");
    assert_eq!(attribute(&dom, "data-aggregate-video"), "0");
    assert_eq!(attribute(&dom, "data-aggregate-audio"), "64000");
}

#[test]
fn audio_bitrate_hints_are_reflected_without_the_native_encoder_clamp() {
    for bitrate in [0, 9, 1_000_000] {
        let options = format!("{{mimeType: 'audio/ogg', audioBitsPerSecond: {bitrate}}}");
        let (dom, mut runtime, request) = start(&options, "");
        run(
            &dom,
            &mut runtime,
            r#"
            document.body.setAttribute('data-constructor-rate', String(recorder.audioBitsPerSecond));
            recorder.start();
            document.body.setAttribute('data-started-rate', String(recorder.audioBitsPerSecond));
        "#,
        );
        assert_eq!(
            attribute(&dom, "data-constructor-rate"),
            bitrate.to_string()
        );
        assert_eq!(attribute(&dom, "data-started-rate"), bitrate.to_string());
        // The encoder bounds its target at open without rewriting the public
        // constructor hint or claiming that its encoded rate exactly matches it.
        capture(&mut runtime, request, 1, 48_000, 1, 960);
        run(&dom, &mut runtime, "recorder.stop();");
        decoded(&dom, 1, 960);
    }
}

#[test]
fn aggregate_bitrate_hint_overrides_explicit_audio_even_when_zero() {
    let (dom, mut runtime, _) = start("{mimeType: 'audio/ogg'}", "");
    run(
        &dom,
        &mut runtime,
        r#"
        const rates = [];
        const videos = [];
        for (const bitrate of [0, 9, 1000000]) {
            const candidate = new MediaRecorder(stream, {mimeType: 'audio/ogg',
                audioBitsPerSecond: 24000, videoBitsPerSecond: 12000,
                bitsPerSecond: bitrate});
            rates.push(candidate.audioBitsPerSecond);
            videos.push(candidate.videoBitsPerSecond);
        }
        document.body.setAttribute('data-aggregate-rates', rates.join(','));
        document.body.setAttribute('data-aggregate-videos', videos.join(','));
    "#,
    );
    assert_eq!(attribute(&dom, "data-aggregate-rates"), "0,9,1000000");
    assert_eq!(attribute(&dom, "data-aggregate-videos"), "0,0,0");
}

#[test]
fn timeslice_conversion_errors_precede_active_and_unsupported_stream_checks() {
    let (dom, mut runtime, request) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    run(
        &dom,
        &mut runtime,
        r#"
        const empty = new MediaRecorder(new MediaStream(), {mimeType: 'audio/ogg'});
        const conflicting = new MediaRecorder(stream, {mimeType: 'audio/ogg',
            videoKeyFrameIntervalCount: 1, videoKeyFrameIntervalDuration: 100});
        const conversionErrors = [];
        const algorithmErrors = [];
        for (const candidate of [recorder, empty, conflicting]) {
            for (const value of [Symbol('slice'), 1n]) {
                try {candidate.start(value); conversionErrors.push('accepted');}
                catch (error) {conversionErrors.push(error.name);}
            }
            try {candidate.start(100); algorithmErrors.push('accepted');}
            catch (error) {algorithmErrors.push(error.name);}
        }
        document.body.setAttribute('data-timeslice-errors', conversionErrors.join(','));
        document.body.setAttribute('data-start-errors', algorithmErrors.join(','));
        document.body.setAttribute('data-active-state', recorder.state);
    "#,
    );
    assert_eq!(
        attribute(&dom, "data-timeslice-errors"),
        "TypeError,TypeError,TypeError,TypeError,TypeError,TypeError"
    );
    assert_eq!(
        attribute(&dom, "data-start-errors"),
        "InvalidStateError,NotSupportedError,NotSupportedError"
    );
    assert_eq!(attribute(&dom, "data-active-state"), "recording");
    capture(&mut runtime, request, 1, 48_000, 1, 960);
    run(&dom, &mut runtime, "recorder.stop();");
    decoded(&dom, 1, 960);
}

#[test]
fn timeslice_is_converted_once_before_algorithm_checks_but_after_receiver_brand() {
    let (dom, mut runtime, _) = start("{mimeType: 'audio/ogg'}", "recorder.start();");
    run(
        &dom,
        &mut runtime,
        r#"
        const empty = new MediaRecorder(new MediaStream(), {mimeType: 'audio/ogg'});
        let conversions = 0;
        const value = {valueOf() {conversions++; return 100.9;}};
        const names = [];
        for (const candidate of [recorder, empty]) {
            try {candidate.start(value); names.push('accepted');}
            catch (error) {names.push(error.name);}
        }
        let brandConversions = 0;
        try {MediaRecorder.prototype.start.call({}, {valueOf() {
            brandConversions++; return 100;
        }}); names.push('accepted');}
        catch (error) {names.push(error.name);}
        document.body.setAttribute('data-timeslice-conversions', String(conversions));
        document.body.setAttribute('data-brand-conversions', String(brandConversions));
        document.body.setAttribute('data-converted-start-errors', names.join(','));
        recorder.stop();
    "#,
    );
    assert_eq!(attribute(&dom, "data-timeslice-conversions"), "2");
    assert_eq!(attribute(&dom, "data-brand-conversions"), "0");
    assert_eq!(
        attribute(&dom, "data-converted-start-errors"),
        "InvalidStateError,NotSupportedError,TypeError"
    );
}
