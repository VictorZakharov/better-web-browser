    // User Timing Level 3: convert dictionaries once before running the algorithms.
    function timingBrand(receiver, type) {
        const state = brand(entryState, receiver);
        if (state.entryType !== type) throw new TypeError('Illegal invocation');
        return state;
    }
    const markOptions = input => {
        const options = dictionary(input), detail = options.detail, startTime = options.startTime;
        return {detail, startTime: startTime === undefined ? undefined : finite(startTime)};
    };
    const markOrTime = value => typeof value === 'number' ? finite(value) : domString(value);
    const measureOptions = input => {
        const options = dictionary(input), detail = options.detail;
        let duration = options.duration;
        if (duration !== undefined) duration = finite(duration);
        let end = options.end;
        if (end !== undefined) end = markOrTime(end);
        let start = options.start;
        if (start !== undefined) start = markOrTime(start);
        return {detail, duration, end, start};
    };
    class PerformanceMark extends PerformanceEntry {
        constructor(name, options = {}) {
            requireArgument(arguments.length);
            name = domString(name); options = markOptions(options);
            if (isWindow && legacyNames.has(name)) throw new DOMException('Reserved timing name', 'SyntaxError');
            const startTime = options.startTime === undefined ? now() : options.startTime;
            if (startTime < 0) throw new TypeError('Mark startTime must be non-negative');
            super(token, {name, entryType: 'mark', startTime, duration: 0, detail: cloneDetail(options.detail)});
        }
        get detail() { return timingBrand(this, 'mark').detail; }
        toJSON() { return {...timingBrand(this, 'mark')}; }
    }
    class PerformanceMeasure extends PerformanceEntry {
        constructor(secret, state) {
            if (secret !== token) throw new TypeError('Illegal constructor');
            super(token, state);
        }
        get detail() { return timingBrand(this, 'measure').detail; }
        toJSON() { return {...timingBrand(this, 'measure')}; }
    }
    class Performance extends EventTarget {
        constructor(secret) {
            if (secret !== token) throw new TypeError('Illegal constructor');
            super(); performanceBrand.add(this);
        }
        get timeOrigin() { checkPerformance(this); return timeOrigin; }
        now() { checkPerformance(this); return now(); }
        toJSON() {
            checkPerformance(this);
            return isWindow ? {timeOrigin, timing: legacyTiming.toJSON()} : {timeOrigin};
        }
        getEntries() { checkPerformance(this); return filterEntries(entries); }
        getEntriesByType(type) {
            checkPerformance(this); requireArgument(arguments.length);
            return filterEntries(entries, undefined, domString(type));
        }
        getEntriesByName(name, type = undefined) {
            checkPerformance(this); requireArgument(arguments.length);
            name = domString(name); type = type === undefined ? undefined : domString(type);
            return filterEntries(entries, name, type);
        }
        mark(name, options = {}) {
            checkPerformance(this); requireArgument(arguments.length);
            const entry = new PerformanceMark(name, options);
            publishEntry(entry); return entry;
        }
        measure(name, startOrOptions = {}, endMark = undefined) {
            checkPerformance(this); requireArgument(arguments.length);
            name = domString(name);
            const options = startOrOptions == null || typeof startOrOptions === 'object' || typeof startOrOptions === 'function'
                ? measureOptions(startOrOptions) : domString(startOrOptions);
            endMark = endMark === undefined ? undefined : domString(endMark);
            const dict = typeof options === 'object';
            if (dict && Object.values(options).some(value => value !== undefined)) {
                if (endMark !== undefined || (options.start === undefined && options.end === undefined) ||
                    (options.start !== undefined && options.end !== undefined && options.duration !== undefined))
                    throw new TypeError('Invalid measure options combination');
            }
            let end;
            if (endMark !== undefined) end = timestamp(endMark);
            else if (dict && options.end !== undefined) end = timestamp(options.end);
            else if (dict && options.start !== undefined && options.duration !== undefined)
                end = timestamp(options.start) + timestamp(options.duration);
            else end = now();
            let start;
            if (!dict) start = timestamp(options);
            else if (options.start !== undefined) start = timestamp(options.start);
            else if (options.duration !== undefined && options.end !== undefined)
                start = timestamp(options.end) - timestamp(options.duration);
            else start = 0;
            // A negative derived start/duration is valid; supplied timestamps are nonnegative.
            const entry = new PerformanceMeasure(token, {name, entryType: 'measure', startTime: start,
                duration: end - start, detail: dict ? cloneDetail(options.detail) : null});
            publishEntry(entry); return entry;
        }
        clearMarks(name = undefined) {
            checkPerformance(this); clearEntries('mark', name === undefined ? undefined : domString(name));
        }
        clearMeasures(name = undefined) {
            checkPerformance(this); clearEntries('measure', name === undefined ? undefined : domString(name));
        }
        clearResourceTimings() { checkPerformance(this); }
        setResourceTimingBufferSize(size) {
            checkPerformance(this); requireArgument(arguments.length); void (+size >>> 0);
        }
    }
    if (isWindow) Object.defineProperty(Performance.prototype, 'timing', {
        configurable: true, enumerable: true, get() { checkPerformance(this); return legacyTiming; }
    });
