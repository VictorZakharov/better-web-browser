    // Retain bounded lifecycle evidence without media bytes, URLs, or per-frame log traffic.
    // Player applications can handle failures internally without emitting a console error.
    const mediaDiagnosticCounts = new WeakMap();
    const mediaDiagnosticTimes = new WeakMap();
    const traceMediaCallsite = element => {
        const stack = String(new Error().stack || '').split('\n').slice(2, 12).join(' | ')
            .replace(/(?:https?|file):\/\/[^\s)]+/g, value => {
                const position = value.match(/:\d+:\d+$/);
                return '[script]' + (position ? position[0] : '');
            });
        traceMediaLifecycle(element, 'load caller=' + stack.slice(0, 512));
    };
    const traceMediaClock = (element, input) => {
        const bucket = Math.floor(Number(input.currentTime) / 5);
        if (mediaDiagnosticTimes.get(element) === bucket) return;
        mediaDiagnosticTimes.set(element, bucket);
        traceMediaLifecycle(element, 'clock', input.currentTime, input.duration);
    };
    const traceMediaLifecycle = (element, event, position = '', duration = '') => {
        const count = event === 'seek:waiting' || event === 'seek:ready'
            ? 0 : mediaDiagnosticCounts.get(element) || 0;
        const critical = event === 'seek:waiting' || event === 'seek:ready'
            || event === 'buffer:abort' || event === 'request:seek'
            || event === 'response:seeked' || event === 'response:media-error'
            || event === 'request:reset' || event.startsWith('load caller=');
        // Keep late seek/error evidence after repetitive append messages reach their
        // per-element quota. The host independently retains only a bounded tail.
        if (count >= 128 && !critical) return;
        mediaDiagnosticCounts.set(element, Math.min(128, count + 1));
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
