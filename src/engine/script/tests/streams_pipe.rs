use super::*;

#[test]
fn pipe_options_preserve_destination_and_propagate_failures() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const output = [];
                const destination = new WritableStream({
                    write(value) { output.push(value); },
                    close() { output.push('closed'); }
                });
                await ReadableStream.from([1, 2]).pipeTo(destination, { preventClose: true });
                const writer = destination.getWriter();
                await writer.write(3);
                await writer.close();
                writer.releaseLock();

                const sourceFailure = new Error('source');
                let aborted = '', observed = '';
                const broken = new ReadableStream({ pull(controller) { controller.error(sourceFailure); } });
                try {
                    await broken.pipeTo(new WritableStream({ abort(reason) { aborted = reason.message; } }));
                } catch (error) { observed = error.message; }

                const preserved = new WritableStream({ abort() { output.push('bad-abort'); } });
                try {
                    await new ReadableStream({ pull(controller) { controller.error('failed'); } })
                        .pipeTo(preserved, { preventAbort: true });
                } catch (_) {}
                document.querySelector('output').textContent = [
                    output.join(','), aborted, observed, preserved.getWriter().desiredSize
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1,2,3,closed|source|source|1"
    );
}

#[test]
fn destination_failure_cancels_source_unless_prevented() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const cancelled = [];
                const makeSource = () => new ReadableStream({
                    pull(controller) { controller.enqueue(1); },
                    cancel(reason) { cancelled.push(reason.message); }
                });
                const makeDestination = () => new WritableStream({
                    write() { throw new Error('sink'); }
                });
                const observed = [];
                for (const preventCancel of [false, true]) {
                    try { await makeSource().pipeTo(makeDestination(), { preventCancel }); }
                    catch (error) { observed.push(error.message); }
                }
                document.querySelector('output').textContent =
                    observed.join(',') + '|' + cancelled.join(',');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "sink,sink|sink"
    );
}

#[test]
fn writable_queue_exposes_backpressure_and_drain() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                let release;
                const blocked = new Promise(resolve => { release = resolve; });
                const sink = new WritableStream({ write() { return blocked; } },
                    new ByteLengthQueuingStrategy({ highWaterMark: 4 }));
                const writer = sink.getWriter();
                const initial = writer.desiredSize;
                const write = writer.write(new Uint8Array(3));
                const queued = writer.desiredSize;
                const write2 = writer.write(new Uint8Array(2));
                const full = writer.desiredSize;
                let ready = false;
                writer.ready.then(() => { ready = true; });
                await Promise.resolve();
                const pending = ready;
                release();
                await Promise.all([write, write2]);
                await writer.ready;
                document.querySelector('output').textContent =
                    [initial, queued, full, pending, ready, writer.desiredSize].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "4|1|-1|false|true|4"
    );
}

#[test]
fn abort_signal_stops_pipe_and_releases_both_locks() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const aborter = new AbortController();
                let cancelled = '', aborted = '';
                const source = new ReadableStream({ cancel(reason) { cancelled = reason; } });
                const destination = new WritableStream({ abort(reason) { aborted = reason; } });
                const pipe = source.pipeTo(destination, { signal: aborter.signal });
                aborter.abort('stop');
                let rejection = '';
                try { await pipe; } catch (error) { rejection = error; }
                document.querySelector('output').textContent = [
                    cancelled, aborted, rejection, source.locked, destination.locked
                ].join('|');
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "stop|stop|stop|false|false"
    );
}
