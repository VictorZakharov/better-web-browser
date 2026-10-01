use super::*;

#[test]
fn panners_compressor_and_convolver_share_stereo_input_constraints() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const types = [PannerNode, StereoPannerNode, DynamicsCompressorNode, ConvolverNode];
        for (const Type of types) {
            const node = new Type(context);
            if (node.channelCount !== 2 || node.channelCountMode !== 'clamped-max' ||
                node.channelInterpretation !== 'speakers') throw Error(Type.name + ' defaults');
            for (const action of [
                () => { node.channelCount = 3; },
                () => { node.channelCountMode = 'max'; },
                () => new Type(context, {channelCount: 3}),
                () => new Type(context, {channelCountMode: 'max'})
            ]) {
                let name;
                try { action(); } catch (error) { name = error.name; }
                if (name !== 'NotSupportedError') throw Error(Type.name + ' channel restriction');
            }
            if (node.channelCount !== 2 || node.channelCountMode !== 'clamped-max')
                throw Error('rejected restricted setter changed state');
            node.channelCount = 1;
            node.channelCountMode = 'explicit';
            node.channelInterpretation = 'discrete';
            const constructed = new Type(context, {channelCount: 1,
                channelCountMode: 'explicit', channelInterpretation: 'discrete'});
            if (node.channelCount !== 1 || constructed.channelCount !== 1 ||
                constructed.channelCountMode !== 'explicit' ||
                constructed.channelInterpretation !== 'discrete')
                throw Error('restricted node rejected valid mono/discrete options');
        }
        console.log('restricted stereo channels passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: restricted stereo channels passed"]);
}

#[test]
fn splitter_channel_settings_are_fixed_and_discrete() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const splitter = new ChannelSplitterNode(context, {numberOfOutputs: 4});
        if (splitter.channelCount !== 4 || splitter.channelCountMode !== 'explicit' ||
            splitter.channelInterpretation !== 'discrete') throw Error('splitter defaults');
        for (const action of [
            () => { splitter.channelCount = 2; },
            () => { splitter.channelCountMode = 'max'; },
            () => { splitter.channelCountMode = 'clamped-max'; },
            () => { splitter.channelInterpretation = 'speakers'; },
            () => new ChannelSplitterNode(context, {numberOfOutputs: 4, channelCount: 2}),
            () => new ChannelSplitterNode(context, {channelCountMode: 'max'}),
            () => new ChannelSplitterNode(context, {channelInterpretation: 'speakers'})
        ]) {
            let name;
            try { action(); } catch (error) { name = error.name; }
            if (name !== 'InvalidStateError') throw Error('splitter restriction: ' + name);
        }
        splitter.channelCount = 4;
        splitter.channelCountMode = 'explicit';
        splitter.channelInterpretation = 'discrete';
        if (splitter.channelCount !== 4) throw Error('fixed no-op setter changed splitter');
        console.log('splitter channel constraints passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: splitter channel constraints passed"]
    );
}

#[test]
fn merger_inputs_are_fixed_mono_but_interpretation_is_mutable() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const merger = new ChannelMergerNode(context, {numberOfInputs: 4});
        if (merger.channelCount !== 1 || merger.channelCountMode !== 'explicit' ||
            merger.channelInterpretation !== 'speakers') throw Error('merger defaults');
        for (const action of [
            () => { merger.channelCount = 2; },
            () => { merger.channelCountMode = 'max'; },
            () => new ChannelMergerNode(context, {channelCount: 2}),
            () => new ChannelMergerNode(context, {channelCountMode: 'clamped-max'})
        ]) {
            let name;
            try { action(); } catch (error) { name = error.name; }
            if (name !== 'InvalidStateError') throw Error('merger restriction: ' + name);
        }
        merger.channelInterpretation = 'discrete';
        merger.channelCount = 1;
        merger.channelCountMode = 'explicit';
        if (merger.channelInterpretation !== 'discrete' || merger.numberOfInputs !== 4)
            throw Error('merger interpretation or independent port count');
        console.log('merger channel constraints passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: merger channel constraints passed"]);
}

#[test]
fn offline_destination_has_fixed_count_and_explicit_mode() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(4, 128, 8000);
        const destination = context.destination;
        if (destination.channelCount !== 4 || destination.maxChannelCount !== 4 ||
            destination.channelCountMode !== 'explicit') throw Error('offline destination defaults');
        for (const action of [
            () => { destination.channelCount = 2; },
            () => { destination.channelCount = 0; },
            () => { destination.channelCountMode = 'max'; },
            () => { destination.channelCountMode = 'clamped-max'; }
        ]) {
            let name;
            try { action(); } catch (error) { name = error.name; }
            if (name !== 'InvalidStateError') throw Error('offline destination restriction: ' + name);
        }
        destination.channelCount = 4;
        destination.channelCountMode = 'explicit';
        destination.channelInterpretation = 'discrete';
        if (destination.channelCount !== 4 || destination.channelInterpretation !== 'discrete')
            throw Error('offline destination accepted settings');
        console.log('offline destination constraints passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: offline destination constraints passed"]
    );
}

#[test]
fn live_destination_validates_device_count_without_changing_offline_policy() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new AudioContext();
        const destination = context.destination;
        for (const value of [0, destination.maxChannelCount + 1, 33, -1]) {
            let name;
            try { destination.channelCount = value; } catch (error) { name = error.name; }
            if (name !== 'IndexSizeError') throw Error('live device count restriction: ' + name);
        }
        destination.channelCount = 1;
        destination.channelCountMode = 'clamped-max';
        destination.channelInterpretation = 'discrete';
        if (destination.channelCount !== 1 || destination.channelCountMode !== 'clamped-max')
            throw Error('live destination settings');
        context.close().then(() => console.log('live destination constraints passed'));
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: live destination constraints passed"]
    );
}

#[test]
fn rejected_channel_options_do_not_consume_graph_node_slots() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        for (let i = 0; i < 300; ++i) {
            for (const Type of [GainNode, StereoPannerNode, ChannelMergerNode]) {
                let rejected = false;
                try { new Type(context, {channelCount: 0}); }
                catch (error) { rejected = error.name === 'NotSupportedError' ||
                    error.name === 'InvalidStateError'; }
                if (!rejected) throw Error('invalid channel count accepted');
            }
        }
        for (let i = 0; i < 255; ++i) context.createGain();
        let name;
        try { context.createGain(); } catch (error) { name = error.name; }
        if (name !== 'NotSupportedError') throw Error('graph node quota not enforced');
        console.log('channel constructor admission passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: channel constructor admission passed"]
    );
}
