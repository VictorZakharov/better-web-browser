use super::*;

#[test]
fn offline_suspension_rounds_up_to_quantum_and_accepts_graph_edits() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 384, 8000);
        const source = context.createConstantSource();
        source.offset.value = 0;
        source.connect(context.destination);
        source.start();
        const pause = context.suspend(129 / 8000);
        const rendering = context.startRendering();
        pause.then(() => {
            if (context.state !== 'suspended' || context.currentTime !== 256 / 8000)
                throw Error('offline suspension did not round to next quantum');
            source.offset.value = 1;
            return context.resume();
        });
        rendering.then(buffer => {
            const samples = buffer.getChannelData(0);
            if (context.state !== 'closed' || samples[0] !== 0 ||
                samples[255] !== 0 || samples[256] !== 1 || samples[383] !== 1)
                throw Error('graph edit while suspended did not affect remaining PCM');
            console.log('offline suspend resume passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: offline suspend resume passed"]);
}

#[test]
fn offline_suspension_rejects_duplicate_and_out_of_range_times() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const valid = context.suspend(1 / 8000);
        const errors = [context.suspend(2 / 8000),
            context.suspend(256 / 8000), context.suspend(-1), context.resume()];
        valid.then(() => context.resume());
        Promise.all(errors.map(promise => promise.then(() => 'resolved',
            error => error.name))).then(names => {
            if (names.join(',') !== 'InvalidStateError,InvalidStateError,' +
                'InvalidStateError,InvalidStateError')
                throw Error('offline suspension validation');
            console.log('offline suspension validation passed');
        });
        context.startRendering();
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: offline suspension validation passed"]
    );
}
