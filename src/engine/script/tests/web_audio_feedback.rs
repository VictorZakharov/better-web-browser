use super::*;

#[test]
fn delayed_feedback_renders_geometric_echoes_at_quantum_boundaries() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 640, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        const source = new AudioBufferSourceNode(context, {buffer});
        const delay = new DelayNode(context, {maxDelayTime: 0.01, delayTime: 0});
        const feedback = new GainNode(context, {gain: 0.5});
        source.connect(delay);
        delay.connect(feedback).connect(delay);
        delay.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            for (let i = 0; i < pcm.length; ++i) {
                const expected = i > 0 && i % 128 === 0 ?
                    Math.pow(0.5, i / 128 - 1) : 0;
                if (Math.abs(pcm[i] - expected) > 0.000001)
                    throw Error('feedback sample ' + i + ': ' + pcm[i]);
            }
            console.log('quantum feedback passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: quantum feedback passed"]);
}

#[test]
fn residual_zero_delay_cycles_are_muted_without_silencing_independent_input() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const source = new ConstantSourceNode(context, {offset: 0.25});
        const first = context.createGain(), second = context.createGain();
        source.connect(first);
        first.connect(second).connect(first);
        second.connect(context.destination);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            if (!pcm.every(sample => sample === 0.25))
                throw Error('residual cycle leaked or muted independent route');
            console.log('cycle isolation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: cycle isolation passed"]);
}

#[test]
fn parameter_and_listener_cycles_mute_without_recursive_rendering() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 256, 8000);
        const source = new ConstantSourceNode(context, {offset: 1});
        const gain = context.createGain(), panner = context.createPanner();
        source.connect(gain).connect(gain.gain);
        gain.connect(context.destination);
        source.connect(panner).connect(context.listener.positionX);
        panner.connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            for (let channel = 0; channel < 2; ++channel)
                if (!result.getChannelData(channel).every(sample => sample === 0))
                    throw Error('parameter/listener cycle must be silent');
            console.log('control cycles passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: control cycles passed"]);
}
