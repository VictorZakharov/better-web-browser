use super::*;

#[test]
fn intermediate_gain_keeps_surround_until_destination_downmix() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(6, 128, 8000);
        buffer.getChannelData(2).fill(0.25);
        buffer.getChannelData(3).fill(1); // LFE never contributes to stereo.
        buffer.getChannelData(4).fill(0.5);
        const source = new AudioBufferSourceNode(context, {buffer});
        const gain = new GainNode(context, {channelCount: 1});
        const splitter = new ChannelSplitterNode(context, {numberOfOutputs: 6});
        const merger = new ChannelMergerNode(context, {numberOfInputs: 2});
        source.connect(gain).connect(splitter);
        splitter.connect(merger, 3, 0);
        splitter.connect(merger, 4, 1);
        merger.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            if (!result.getChannelData(0).every(value => value === 1) ||
                !result.getChannelData(1).every(value => value === 0.5))
                throw Error('max mode pre-downmixed an intermediate surround bus');
            console.log('intermediate surround passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: intermediate surround passed"]);
}

#[test]
fn max_clamped_max_and_explicit_have_distinct_computed_bus_widths() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        async function render(mode) {
            const context = new OfflineAudioContext(4, 128, 8000);
            context.destination.channelInterpretation = 'discrete';
            const buffer = context.createBuffer(4, 128, 8000);
            for (let channel = 0; channel < 4; ++channel)
                buffer.getChannelData(channel).fill((channel + 1) / 8);
            const source = new AudioBufferSourceNode(context, {buffer});
            const gain = new GainNode(context, {channelCount: 2,
                channelCountMode: mode, channelInterpretation: 'discrete'});
            source.connect(gain).connect(context.destination);
            source.start();
            return context.startRendering();
        }
        Promise.all(['max', 'clamped-max', 'explicit'].map(render)).then(results => {
            const expected = [[0.125,0.25,0.375,0.5], [0.125,0.25,0,0], [0.125,0.25,0,0]];
            for (let mode = 0; mode < results.length; ++mode)
                for (let channel = 0; channel < 4; ++channel)
                    if (!results[mode].getChannelData(channel).every(value =>
                        value === expected[mode][channel]))
                        throw Error('channel mode ' + mode + ' channel ' + channel);
            console.log('channel modes passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel modes passed"]);
}

#[test]
fn explicit_speakers_and_discrete_upmix_mono_differently() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function render(interpretation) {
            const context = new OfflineAudioContext(6, 128, 8000);
            context.destination.channelInterpretation = 'discrete';
            const source = new ConstantSourceNode(context, {offset: 0.5});
            const gain = new GainNode(context, {channelCount: 6,
                channelCountMode: 'explicit', channelInterpretation: interpretation});
            source.connect(gain).connect(context.destination);
            source.start();
            return context.startRendering();
        }
        Promise.all([render('speakers'), render('discrete')]).then(([speaker, discrete]) => {
            for (let channel = 0; channel < 6; ++channel) {
                const a = channel === 2 ? 0.5 : 0;
                const b = channel === 0 ? 0.5 : 0;
                if (!speaker.getChannelData(channel).every(value => value === a) ||
                    !discrete.getChannelData(channel).every(value => value === b))
                    throw Error('mono explicit interpretation channel ' + channel);
            }
            console.log('mono interpretation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: mono interpretation passed"]);
}

#[test]
fn clamped_max_does_not_upmix_a_narrower_source_to_channel_count() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(4, 128, 8000);
        context.destination.channelInterpretation = 'discrete';
        const source = new ConstantSourceNode(context, {offset: 0.75});
        const gain = new GainNode(context, {channelCount: 4, channelCountMode: 'clamped-max'});
        source.connect(gain).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            for (let channel = 0; channel < 4; ++channel)
                if (!result.getChannelData(channel).every(value =>
                    value === (channel === 0 ? 0.75 : 0)))
                    throw Error('clamped-max upmixed an already mono source');
            console.log('clamped width passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: clamped width passed"]);
}

#[test]
fn heterogeneous_connections_are_mixed_before_summing() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function render(interpretation) {
            const context = new OfflineAudioContext(2, 128, 8000);
            const mono = new ConstantSourceNode(context, {offset: 0.25});
            const buffer = context.createBuffer(4, 128, 8000);
            buffer.getChannelData(0).fill(0.5);
            buffer.getChannelData(2).fill(0.5);
            const quad = new AudioBufferSourceNode(context, {buffer});
            const mix = new GainNode(context, {channelCount: 2,
                channelCountMode: 'explicit', channelInterpretation: interpretation});
            quad.connect(mix); mono.connect(mix);
            mix.connect(context.destination);
            quad.start(); mono.start();
            return context.startRendering();
        }
        Promise.all([render('speakers'), render('discrete')]).then(([speakers, discrete]) => {
            if (speakers.getChannelData(0)[0] !== 0.75 ||
                speakers.getChannelData(1)[0] !== 0.25 ||
                discrete.getChannelData(0)[0] !== 0.75 ||
                discrete.getChannelData(1)[0] !== 0)
                throw Error('each connection needs its own mixing matrix');
            console.log('connection mixing passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: connection mixing passed"]);
}

#[test]
fn destination_interpretation_changes_the_final_mixing_stage() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        function render(interpretation) {
            const context = new OfflineAudioContext(1, 128, 8000);
            context.destination.channelInterpretation = interpretation;
            const buffer = context.createBuffer(2, 128, 8000);
            buffer.getChannelData(0).fill(0.25);
            buffer.getChannelData(1).fill(0.75);
            const source = new AudioBufferSourceNode(context, {buffer});
            source.connect(context.destination); source.start();
            return context.startRendering();
        }
        Promise.all([render('speakers'), render('discrete')]).then(([a, b]) => {
            if (!a.getChannelData(0).every(value => value === 0.5) ||
                !b.getChannelData(0).every(value => value === 0.25))
                throw Error('destination ignored channelInterpretation');
            console.log('destination interpretation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: destination interpretation passed"]);
}
