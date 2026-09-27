use super::*;

#[test]
fn audio_buffer_readonly_attributes_are_branded_prototype_accessors() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const buffer = new AudioBuffer({numberOfChannels: 2, length: 8, sampleRate: 8000});
        for (const [name, expected] of [
            ['numberOfChannels', 2], ['length', 8],
            ['sampleRate', 8000], ['duration', 8 / 8000]
        ]) {
            const descriptor = Object.getOwnPropertyDescriptor(AudioBuffer.prototype, name);
            if (Object.hasOwn(buffer, name) || !descriptor ||
                typeof descriptor.get !== 'function' || descriptor.set !== undefined ||
                !descriptor.enumerable || !descriptor.configurable ||
                descriptor.get.call(buffer) !== expected)
                throw Error(name + ' is not a readonly prototype accessor');
            let branded = false;
            try { descriptor.get.call({}); }
            catch (error) { branded = error instanceof TypeError; }
            if (!branded) throw Error(name + ' getter accepted a foreign receiver');
        }
        console.log('audio buffer descriptors passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: audio buffer descriptors passed"]);
}

#[test]
fn dictionary_channel_defaults_are_one_but_positional_channels_remain_required() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
        const context = new OfflineAudioContext({length: 128, sampleRate: 8000});
        if (buffer.numberOfChannels !== 1 || context.numberOfChannels !== 1)
            throw Error('dictionary default channels must be one');
        let positionalRejected = false;
        try { new OfflineAudioContext(undefined, 128, 8000); }
        catch (error) { positionalRejected = error.name === 'NotSupportedError'; }
        if (!positionalRejected) throw Error('positional channels should be required');
        console.log('dictionary channel defaults passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: dictionary channel defaults passed"]);
}

#[test]
fn failed_node_options_do_not_consume_the_graph_node_budget() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const bad = [
            () => new GainNode(context, {gain: NaN}),
            () => new OscillatorNode(context, {frequency: NaN}),
            () => new AudioBufferSourceNode(context, {playbackRate: NaN}),
            () => new ConstantSourceNode(context, {offset: NaN}),
            () => new StereoPannerNode(context, {pan: NaN}),
            () => new BiquadFilterNode(context, {Q: NaN}),
        ];
        for (let attempt = 0; attempt < 300; attempt++) {
            let rejected = false;
            try { bad[attempt % bad.length](); }
            catch (error) { rejected = error instanceof TypeError; }
            if (!rejected) throw Error('bad options were not rejected');
        }
        const source = context.createConstantSource();
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            if (buffer.getChannelData(0)[0] !== 1)
                throw Error('failed constructors damaged a valid graph');
            console.log('failed node construction budget passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: failed node construction budget passed"]
    );
}
