use super::*;

#[test]
fn delay_node_renders_integer_sample_offset_and_tail() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 1, 8000);
        buffer.getChannelData(0)[0] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const delay = context.createDelay(0.1);
        delay.delayTime.value = 2 / 8000;
        source.connect(delay).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(pcm[0], 0) || !near(pcm[1], 0) ||
                !near(pcm[2], 1) || !near(pcm[3], 0))
                throw Error('integer sample delay or post-source tail was lost');
            console.log('delay integer and tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: delay integer and tail passed"]);
}

#[test]
fn delay_node_interpolates_fractional_samples_and_passes_zero_delay() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        async function render(delayTime) {
            const context = new OfflineAudioContext(1, 128, 8000);
            const buffer = context.createBuffer(1, 1, 8000);
            buffer.getChannelData(0)[0] = 1;
            const source = context.createBufferSource();
            source.buffer = buffer;
            const delay = context.createDelay();
            delay.delayTime.value = delayTime;
            source.connect(delay).connect(context.destination);
            source.start();
            return (await context.startRendering()).getChannelData(0);
        }
        Promise.all([render(0), render(0.5 / 8000)]).then(([zero, half]) => {
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(zero[0], 1) || !near(zero[1], 0) ||
                !near(half[0], 0.5) || !near(half[1], 0.5) || !near(half[2], 0))
                throw Error('fractional or zero delay PCM mismatch');
            console.log('delay interpolation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: delay interpolation passed"]);
}

#[test]
fn delay_node_applies_automation_at_the_exact_sample() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0)[128] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const delay = context.createDelay();
        delay.delayTime.setValueAtTime(1 / 8000, 128 / 8000);
        source.connect(delay).connect(context.destination);
        source.start();
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(pcm[127], 0) || !near(pcm[128], 0) || !near(pcm[129], 1))
                throw Error('delayTime automation missed the sample boundary');
            console.log('delay automation passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: delay automation passed"]);
}

#[test]
fn disconnected_delay_line_keeps_history_until_it_is_connected() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(1, 256, 8000);
        const buffer = context.createBuffer(1, 256, 8000);
        buffer.getChannelData(0)[127] = 1;
        const source = context.createBufferSource();
        source.buffer = buffer;
        const delay = context.createDelay();
        delay.delayTime.value = 2 / 8000;
        source.connect(delay);
        source.start();
        const pause = context.suspend(128 / 8000);
        pause.then(() => {
            delay.connect(context.destination);
            return context.resume();
        });
        context.startRendering().then(result => {
            const pcm = result.getChannelData(0);
            const near = (actual, expected) => Math.abs(actual - expected) < 0.00001;
            if (!near(pcm[127], 0) || !near(pcm[128], 0) || !near(pcm[129], 1))
                throw Error('delay history did not survive a graph edit');
            console.log('delay history passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: delay history passed"]);
}

#[test]
fn delay_node_rejects_invalid_or_unbounded_delay_lines() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context = new OfflineAudioContext(2, 128, 48000);
        const names = [];
        for (const max of [0, 180, 100]) {
            try { context.createDelay(max); names.push('accepted'); }
            catch (error) { names.push(error.name); }
        }
        const delay = context.createDelay(0.5);
        if (names.join(',') !== 'NotSupportedError,NotSupportedError,NotSupportedError' ||
            delay.delayTime.defaultValue !== 0 || delay.delayTime.minValue !== 0 ||
            delay.delayTime.maxValue !== 0.5 || delay.delayTime.automationRate !== 'a-rate')
            throw Error('DelayNode option or memory bounds mismatch');
        console.log('delay bounds passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: delay bounds passed"]);
}
