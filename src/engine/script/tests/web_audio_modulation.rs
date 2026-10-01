use super::*;

#[test]
fn audio_rate_modulation_sums_with_intrinsic_gain_at_each_sample() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const carrier = context.createConstantSource();
        const modulator = context.createConstantSource();
        const gain = new GainNode(context, {gain: 0.5});
        modulator.offset.setValueAtTime(0, 0);
        modulator.offset.linearRampToValueAtTime(1, 4 / 8000);
        carrier.connect(gain).connect(context.destination);
        if (modulator.connect(gain.gain) !== undefined)
            throw Error('AudioParam connect must return undefined');
        carrier.start();
        modulator.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (gain.gain.value !== 0.5 || samples[0] !== 0.5 ||
                samples[2] !== 1 || samples[4] !== 1.5 || samples[127] !== 1.5)
                throw Error('a-rate AudioParam summing changed intrinsic value or PCM');
            console.log('a-rate gain modulation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: a-rate gain modulation passed"]);
}

#[test]
fn oscillator_frequency_receives_modulation_without_changing_intrinsic_value() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const modulator = new ConstantSourceNode(context, {offset: 2000});
        const oscillator = new OscillatorNode(context, {frequency: 0});
        modulator.connect(oscillator.frequency);
        oscillator.connect(context.destination);
        modulator.start();
        oscillator.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (oscillator.frequency.value !== 0 ||
                Math.abs(samples[0]) > 0.00001 ||
                Math.abs(samples[1] - 1) > 0.00001 ||
                Math.abs(samples[2]) > 0.00001 ||
                Math.abs(samples[3] + 1) > 0.00001)
                throw Error('oscillator did not use sample-level frequency modulation');
            console.log('oscillator frequency modulation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: oscillator frequency modulation passed"]
    );
}

#[test]
fn buffer_source_playback_rate_modulation_is_sampled_once_per_quantum() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 512, 8000);
        for (let i = 0; i < 512; i++) buffer.getChannelData(0)[i] = i / 512;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const modulator = context.createConstantSource();
        modulator.offset.setValueAtTime(0, 0);
        modulator.offset.setValueAtTime(1, 1 / 8000);
        modulator.connect(source.playbackRate);
        source.connect(context.destination);
        source.start();
        modulator.start();
        context.startRendering().then(result => {
            const samples = result.getChannelData(0);
            if (samples[127] !== 127 / 512 || samples[128] !== 128 / 512 ||
                samples[129] !== 130 / 512 || source.playbackRate.value !== 1)
                throw Error('k-rate modulation did not step at the quantum boundary');
            console.log('k-rate playback modulation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: k-rate playback modulation passed"]);
}

#[test]
fn duplicate_modulation_edges_are_ignored_and_disconnect_takes_effect() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const carrier = context.createConstantSource();
        const modulator = context.createConstantSource();
        const gain = new GainNode(context, {gain: 0});
        carrier.connect(gain).connect(context.destination);
        modulator.connect(gain.gain);
        modulator.connect(gain.gain);
        carrier.start();
        modulator.start();
        const pause = context.suspend(128 / 8000);
        const rendering = context.startRendering();
        pause.then(() => modulator.disconnect(gain.gain)).then(() => context.resume());
        rendering.then(result => {
            const samples = result.getChannelData(0);
            if (samples[0] !== 1 || samples[127] !== 1 ||
                samples[128] !== 0 || samples[255] !== 0)
                throw Error('duplicate or disconnected param edges changed PCM');
            console.log('modulation disconnect passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: modulation disconnect passed"]);
}

#[test]
fn param_connections_validate_context_output_and_cycles() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const other = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        const gain = context.createGain();
        const errors = [];
        for (const attempt of [
            () => source.connect(other.createGain().gain),
            () => source.connect(gain.gain, 1),
            () => context.destination.connect(gain.gain),
            () => gain.connect(gain.gain),
        ]) {
            try { attempt(); errors.push('none'); }
            catch (error) { errors.push(error.name); }
        }
        source.connect(gain.gain);
        try { gain.connect(source.offset); errors.push('none'); }
        catch (error) { errors.push(error.name); }
        source.disconnect(gain.gain);
        try { source.disconnect(gain.gain); errors.push('none'); }
        catch (error) { errors.push(error.name); }
        if (errors.join(',') !==
            'InvalidAccessError,IndexSizeError,IndexSizeError,' +
            'none,none,InvalidAccessError')
            throw Error('AudioParam routing validation: ' + errors.join(','));
        console.log('AudioParam routing validation passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: AudioParam routing validation passed"]
    );
}

#[test]
fn stereo_modulation_downmixes_and_splitter_output_index_is_honored() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function graph(useSplitter) {
            const context = new OfflineAudioContext(1, 128, 8000);
            const stereo = context.createBuffer(2, 128, 8000);
            stereo.getChannelData(0).fill(0.25);
            stereo.getChannelData(1).fill(0.75);
            const source = context.createBufferSource();
            source.buffer = stereo;
            const carrier = context.createConstantSource();
            const gain = new GainNode(context, {gain: 0});
            carrier.connect(gain).connect(context.destination);
            if (useSplitter) {
                const splitter = context.createChannelSplitter(2);
                source.connect(splitter);
                splitter.connect(gain.gain, 1);
            } else source.connect(gain.gain);
            source.start();
            carrier.start();
            return context.startRendering();
        }
        Promise.all([graph(false), graph(true)]).then(([downmixed, selected]) => {
            if (downmixed.getChannelData(0)[0] !== 0.5 ||
                selected.getChannelData(0)[0] !== 0.75)
                throw Error('modulation mono downmix or indexed output was wrong');
            console.log('modulation channel routing passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: modulation channel routing passed"]);
}
