    // Retain bounded lifecycle evidence without media bytes, URLs, or per-frame log traffic.
    // Player applications can handle failures internally without emitting a console error.
    const mediaDiagnosticCounts = new WeakMap();
    const mediaDiagnosticTimes = new WeakMap();
    const traceMediaClock = (element, input) => {
        const bucket = Math.floor(Number(input.currentTime) / 5);
        if (mediaDiagnosticTimes.get(element) === bucket) return;
        mediaDiagnosticTimes.set(element, bucket);
        traceMediaLifecycle(element, 'clock', input.currentTime, input.duration);
    };
    const traceMediaLifecycle = (element, event, position = '', duration = '') => {
        const count = mediaDiagnosticCounts.get(element) || 0;
        if (count >= 128) return;
        mediaDiagnosticCounts.set(element, count + 1);
        const state = mediaStateFor(element);
        const source = mediaSourceForElement.get(element);
        const ranges = value => Array.from({ length: Math.min(value.length, 4) },
            (_, index) => value.start(index) + '-' + value.end(index)).join(',');
        const buffers = source ? [...source.sourceBuffers].map(buffer =>
            mediaTrackKind(buffer.__type) + ':' + buffer.__bytes + ':'
            + Number(buffer.updating) + ':' + ranges(buffer.buffered)).join(';') : '';
        host('mediaDiagnostic', element.__id, event + ' t=' + state.currentTime
            + ' duration=' + state.duration + ' ready=' + state.readyState
            + ' paused=' + state.paused + ' buffered=' + ranges(state.buffered)
            + ' native=' + position + '/' + duration
            + ' mse=' + (source?.readyState || '-') + ' buffers=' + buffers);
    };
