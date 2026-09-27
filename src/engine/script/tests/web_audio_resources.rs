use super::*;

#[test]
fn nodes_added_after_start_rendering_obey_the_remaining_work_budget() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(32, 60000, 8000);
        context.startRendering();
        let added = 0, rejected = false;
        for (let i = 0; i < 40; i++) {
            try { new GainNode(context); added++; }
            catch (error) {
                rejected = error.name === 'NotSupportedError';
                break;
            }
        }
        // destination + 32 gains = 33 nodes; node 34 exceeds
        // 60,000 frames × 32 channels × 34 > the 64M work ceiling.
        if (added !== 32 || !rejected)
            throw Error('post-start graph growth bypassed work admission: ' + added);
        console.log('dynamic graph work admission passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: dynamic graph work admission passed"]
    );
}

#[test]
fn automation_curve_memory_is_bounded_across_parameters_and_reclaimed() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const first = context.createConstantSource();
        const second = context.createConstantSource();
        const values = new Float32Array(65536);
        for (let i = 0; i < 128; i++) {
            const param = i % 2 ? first.offset : second.offset;
            param.setValueCurveAtTime(values, 1 + 2 * Math.floor(i / 2), 1);
        }
        let rejected = false;
        try { first.offset.setValueCurveAtTime(values, 200, 1); }
        catch (error) { rejected = error.name === 'NotSupportedError'; }
        if (!rejected) throw Error('aggregate curve memory limit was bypassed');
        first.offset.cancelAndHoldAtTime(0);
        second.offset.cancelScheduledValues(0);
        // Both cancellation methods must release all future curve bytes.
        for (let i = 0; i < 128; i++)
            first.offset.setValueCurveAtTime(values, 1 + 2 * i, 1);
        console.log('automation curve memory accounting passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: automation curve memory accounting passed"]
    );
}

#[test]
fn dense_automation_timeline_keeps_exact_sample_values() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const source = context.createConstantSource();
        for (let i = 0; i < 4096; i++)
            source.offset.setValueAtTime(i / 4096, i / 8000);
        source.connect(context.destination);
        source.start();
        context.startRendering().then(buffer => {
            const samples = buffer.getChannelData(0);
            if (samples[0] !== 0 || samples[64] !== 64 / 4096 ||
                samples[127] !== 127 / 4096)
                throw Error('dense event lookup altered exact sample values');
            console.log('dense automation timeline passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: dense automation timeline passed"]);
}

#[test]
fn short_offline_render_waits_for_a_media_task_and_completes_in_order() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 128, 8000);
        const order = [];
        context.oncomplete = event => {
            if (!(event.renderedBuffer instanceof AudioBuffer))
                throw Error('completion event omitted the rendered buffer');
            order.push('complete');
            if (order.join(',') !== 'microtask,promise,complete')
                throw Error('offline completion order: ' + order.join(','));
            console.log('offline media task order passed');
        };
        context.startRendering().then(() => order.push('promise'));
        queueMicrotask(() => {
            if (context.currentTime !== 0 || context.state !== 'running')
                throw Error('short render completed within the calling microtask checkpoint');
            order.push('microtask');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: offline media task order passed"]);
}
