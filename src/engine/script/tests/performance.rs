use super::*;

fn assert_script_passes(script: &str) {
    let (dom, outcome) = execute_html(&format!("<body><script>{script}</script></body>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("PASS")
    );
}

#[test]
fn performance_clock_cloning_and_delivery_ignore_author_api_replacements() {
    assert_script_passes(
        r#"
        const clock = performance.now.bind(performance), before = clock();
        Date.now = () => -1e10;
        performance.now = () => -999;
        structuredClone = () => { throw Error('author clone'); };
        setTimeout = () => { throw Error('author timer'); };
        const detail = {value: 1}; detail.self = detail;
        new PerformanceObserver((list, observer) => {
            const mark = list.getEntries()[0];
            if (mark.startTime < before || mark.detail.value !== 1 || mark.detail.self !== mark.detail)
                throw Error('internal clock or clone');
            if (clock() < before) throw Error('clock regressed');
            observer.disconnect(); document.body.dataset.result = 'PASS';
        }).observe({type:'mark'});
        performance.mark('native', {detail}); detail.value = 9;
        for (let id = 1; id < 100; id++) clearTimeout(id);
    "#,
    );
}

#[test]
fn observer_errors_do_not_abort_other_observers_and_buffered_delivery_is_reentrant() {
    assert_script_passes(
        r#"
        let reported = 0, calls = 0;
        addEventListener('error', event => {
            if (event.message !== 'observer failure' || !event.isTrusted) throw Error('wrong error');
            reported++; event.preventDefault();
        });
        const failing = new PerformanceObserver(() => { failing.disconnect(); throw Error('observer failure'); });
        failing.observe({type:'mark'});
        const good = new PerformanceObserver((list, observer, options) => {
            if (reported !== 1 || list.getEntries().length !== 1) throw Error('delivery aborted');
            calls++;
            if (calls === 1) {
                if (options.droppedEntriesCount !== 0) throw Error('initial dropped count');
                performance.mark('second');
            } else {
                if ('droppedEntriesCount' in options) throw Error('repeated dropped count');
                observer.disconnect();
                try { observer.observe({entryTypes:['mark']}); throw Error('mode reset'); }
                catch (error) { if (error.name !== 'InvalidModificationError') throw error; }
                document.body.dataset.result = 'PASS';
            }
        });
        good.observe({type:'mark'}); performance.mark('first');
    "#,
    );
}

#[test]
fn timing_dictionaries_brands_and_entry_serialization_follow_webidl() {
    assert_script_passes(
        r#"
        const throws = (name, fn) => {
            try { fn(); } catch (error) { if (error.name === name) return; throw error; }
            throw Error('missing '+name);
        };
        let reads = 0;
        const observer = new PerformanceObserver(() => {});
        observer.observe({entryTypes:{get [Symbol.iterator]() {
            reads++; return function*() { yield 'mark'; };
        }}});
        if (reads !== 1) throw Error('iterator read twice');
        const mark = performance.mark('one', {startTime:10});
        const negative = performance.measure('negative', {start:10,end:4});
        const beforeOrigin = performance.measure('before', {end:4,duration:10});
        if (negative.duration !== -6 || beforeOrigin.startTime !== -6) throw Error('derived values');
        if (Object.prototype.toString.call(mark) !== '[object PerformanceMark]') throw Error('tag');
        throws('TypeError', () => Object.getOwnPropertyDescriptor(PerformanceMark.prototype,'detail').get.call(negative));
        throws('DataCloneError', () => structuredClone(mark));
        throws('DataCloneError', () => performance.mark('bad', {detail:()=>{}}));
        throws('TypeError', () => performance.mark('bad', {startTime:NaN}));
        throws('SyntaxError', () => performance.mark('navigationStart'));
        if (performance.getEntriesByName('bad').length) throw Error('published failed mark');
        if (!Object.isFrozen(PerformanceObserver.supportedEntryTypes) ||
            PerformanceObserver.supportedEntryTypes.join() !== 'mark,measure') throw Error('types');
        observer.disconnect(); document.body.dataset.result = 'PASS';
    "#,
    );
}

#[test]
fn worker_performance_observer_uses_its_own_timeline_and_task_queue() {
    use std::sync::Arc;
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.com/timing-worker.js",
        r#"
            const order = ['script'];
            performance.mark('navigationStart', {startTime:1});
            new PerformanceObserver((list, observer) => {
                const entries = list.getEntries();
                if (entries.length !== 1 || entries[0].name !== 'navigationStart') throw Error('worker entries');
                observer.disconnect(); order.push('observer'); postMessage(order.join(','));
            }).observe({type:'mark',buffered:true});
            Promise.resolve().then(() => order.push('microtask'));
            for (let id=1; id<10; id++) clearTimeout(id);
        "#,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(initial.messages.is_empty());
    let mut runtime = runtime.expect("Worker starts");
    let delivered = runtime.advance_time(Duration::ZERO, 8);
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(delivered.messages, ["\"script,microtask,observer\""]);
}

#[test]
fn observer_delivery_is_a_task_with_independent_buffers_and_immutable_entries() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
        const order = [], detail = {nested: {value: 1}};
        const first = new PerformanceObserver(function(list, observer, options) {
            order.push('observer');
            const entries = list.getEntries();
            if (this !== first || observer !== first || options.droppedEntriesCount !== 0 ||
                entries.length !== 2 || entries[0].name !== 'earlier' ||
                !(list instanceof PerformanceObserverEntryList)) throw Error('delivery');
            observer.disconnect();
            document.body.dataset.result = order.join(',');
        });
        first.observe({type:'mark'});
        const second = new PerformanceObserver(() => {throw Error('drained callback')});
        second.observe({entryTypes:['mark']});
        const entry = performance.mark('later', {startTime: 20, detail});
        detail.nested.value = 9;
        entry.startTime = -1; entry.name = 'corrupted';
        if (entry.startTime !== 20 || entry.name !== 'later' || entry.detail.nested.value !== 1)
            throw Error('entry isolation');
        performance.mark('earlier', {startTime: 10});
        if (second.takeRecords().length !== 2 || second.takeRecords().length) throw Error('drain');
        performance.clearMarks();
        Promise.resolve().then(() => order.push('microtask'));
        order.push('script');
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("script,microtask,observer")
    );
}
