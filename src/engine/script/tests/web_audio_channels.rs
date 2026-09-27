use super::*;

#[test]
fn indexed_ports_split_and_recombine_stereo_in_reverse_order() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(2, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        buffer.getChannelData(1)[0] = 0.25;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const splitter = context.createChannelSplitter(2);
        const merger = context.createChannelMerger(2);
        source.connect(splitter);
        splitter.connect(merger, 0, 1);
        splitter.connect(merger, 1, 0);
        merger.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const left = result.getChannelData(0);
            const right = result.getChannelData(1);
            if (left[0] !== 0.25 || right[0] !== 1 ||
                left[1] !== 0 || right[1] !== 0)
                throw Error('indexed channel routing or shared source cache failed');
            console.log('indexed channel swap passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: indexed channel swap passed"]);
}

#[test]
fn splitter_explicitly_pads_missing_mono_channels_with_silence() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const splitter = context.createChannelSplitter(2);
        const merger = context.createChannelMerger(2);
        source.connect(splitter);
        splitter.connect(merger, 0, 0);
        splitter.connect(merger, 1, 1);
        merger.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            if (result.getChannelData(0)[0] !== 1 ||
                result.getChannelData(1)[0] !== 0)
                throw Error('mono input was replicated to an absent splitter channel');
            console.log('explicit splitter channels passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: explicit splitter channels passed"]);
}

#[test]
fn merger_renders_distinct_mono_inputs_and_unconnected_ports() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const left = context.createConstantSource();
        left.offset.value = 0.25;
        const right = context.createConstantSource();
        right.offset.value = 0.75;
        const merger = context.createChannelMerger(2);
        left.connect(merger, 0, 0);
        right.connect(merger, 0, 1);
        merger.connect(context.destination);
        left.start(); right.start();
        context.startRendering().then(result => {
            if (result.getChannelData(0)[0] !== 0.25 ||
                result.getChannelData(1)[0] !== 0.75)
                throw Error('merger mixed independently routed inputs');
            console.log('merger independent inputs passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: merger independent inputs passed"]);
}

#[test]
fn indexed_disconnect_preserves_other_ports_and_validates_indices() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const buffer = context.createBuffer(2, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        buffer.getChannelData(1)[0] = 0.5;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const splitter = new ChannelSplitterNode(context, {numberOfOutputs: 2});
        const merger = new ChannelMergerNode(context, {numberOfInputs: 2});
        source.connect(splitter);
        splitter.connect(merger, 0, 0);
        splitter.connect(merger, 1, 1);
        splitter.connect(merger, 1, 1); // Duplicate edge is a no-op.
        splitter.disconnect(merger, 0, 0);
        let missing, badOutput, badInput;
        try { splitter.disconnect(merger, 0, 0); } catch (error) { missing = error.name; }
        try { splitter.connect(merger, 2, 0); } catch (error) { badOutput = error.name; }
        try { splitter.connect(merger, 1, 2); } catch (error) { badInput = error.name; }
        merger.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            if (missing !== 'InvalidAccessError' || badOutput !== 'IndexSizeError' ||
                badInput !== 'IndexSizeError' || result.getChannelData(0)[0] !== 0 ||
                result.getChannelData(1)[0] !== 0.5)
                throw Error('indexed disconnect or port validation failed');
            console.log('indexed disconnect passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: indexed disconnect passed"]);
}

#[test]
fn channel_port_counts_enforce_supported_range() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 8000);
        const names = [];
        for (const count of [0, 33]) {
            try { context.createChannelSplitter(count); names.push('accepted'); }
            catch (error) { names.push(error.name); }
            try { context.createChannelMerger(count); names.push('accepted'); }
            catch (error) { names.push(error.name); }
        }
        const splitter = context.createChannelSplitter();
        const merger = context.createChannelMerger();
        if (names.join(',') !== Array(4).fill('IndexSizeError').join(',') ||
            splitter.numberOfOutputs !== 6 || merger.numberOfInputs !== 6)
            throw Error('channel port count validation failed');
        console.log('channel port counts passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: channel port counts passed"]);
}
