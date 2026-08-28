    // Performance Timeline and User Timing keep a per-realm entry buffer. Entries returned from
    // retrieval methods are snapshots, while each PerformanceEntry remains immutable.
    const performanceTimeOrigin = Date.now();
    const performanceEntries = [];

    class PerformanceEntry {
        constructor(name, entryType, startTime, duration) {
            this.name = String(name);
            this.entryType = entryType;
            this.startTime = startTime;
            this.duration = duration;
        }
        toJSON() {
            return {
                name: this.name,
                entryType: this.entryType,
                startTime: this.startTime,
                duration: this.duration
            };
        }
    }

    class PerformanceMark extends PerformanceEntry {
        constructor(name, options = {}) {
            options = Object(options || {});
            const startTime = options.startTime === undefined
                ? performance.now()
                : Number(options.startTime);
            if (!Number.isFinite(startTime) || startTime < 0)
                throw new TypeError('Performance mark startTime must be a finite non-negative number');
            super(name, 'mark', startTime, 0);
            this.detail = options.detail ?? null;
        }
        toJSON() { return { ...super.toJSON(), detail: this.detail }; }
    }

    const performanceMeasureToken = {};
    class PerformanceMeasure extends PerformanceEntry {
        constructor(token, name, startTime, duration, detail) {
            if (token !== performanceMeasureToken) throw new TypeError('Illegal constructor');
            super(name, 'measure', startTime, duration);
            this.detail = detail;
        }
        toJSON() { return { ...super.toJSON(), detail: this.detail }; }
    }

    function latestMarkTime(name) {
        name = String(name);
        for (let index = performanceEntries.length - 1; index >= 0; index--) {
            const entry = performanceEntries[index];
            if (entry.entryType === 'mark' && entry.name === name) return entry.startTime;
        }
        throw new DOMException(`The mark '${name}' does not exist`, 'SyntaxError');
    }

    function timestamp(value, fallback) {
        if (value === undefined) return fallback;
        if (typeof value === 'string') return latestMarkTime(value);
        const result = Number(value);
        if (!Number.isFinite(result) || result < 0)
            throw new TypeError('Performance timestamps must be finite non-negative numbers');
        return result;
    }

    class Performance {
        get timeOrigin() { return performanceTimeOrigin; }
        get timing() { return this.__timing ||= { navigationStart: performanceTimeOrigin }; }
        now() { return Math.max(0, Date.now() - performanceTimeOrigin); }
        getEntries() {
            return performanceEntries.slice().sort((left, right) => left.startTime - right.startTime);
        }
        getEntriesByType(type) {
            type = String(type);
            return this.getEntries().filter(entry => entry.entryType === type);
        }
        getEntriesByName(name, type) {
            name = String(name);
            return this.getEntries().filter(entry => entry.name === name &&
                (type === undefined || entry.entryType === String(type)));
        }
        mark(name, options = {}) {
            const entry = new PerformanceMark(name, options);
            performanceEntries.push(entry);
            return entry;
        }
        clearMarks(name) {
            this.__clear('mark', name);
        }
        measure(name, startOrOptions = {}, endMark) {
            let startTime;
            let endTime;
            let detail = null;
            if (typeof startOrOptions === 'string') {
                startTime = latestMarkTime(startOrOptions);
                endTime = endMark === undefined ? this.now() : latestMarkTime(endMark);
            } else {
                const options = Object(startOrOptions || {});
                detail = options.detail ?? null;
                if (options.duration !== undefined) {
                    const duration = timestamp(options.duration, 0);
                    if (options.start !== undefined) {
                        startTime = timestamp(options.start, 0);
                        endTime = startTime + duration;
                    } else {
                        endTime = timestamp(options.end, this.now());
                        startTime = endTime - duration;
                    }
                } else {
                    startTime = timestamp(options.start, 0);
                    endTime = timestamp(options.end, this.now());
                }
            }
            if (startTime < 0 || endTime < startTime)
                throw new TypeError('Performance measure duration cannot be negative');
            const entry = new PerformanceMeasure(
                performanceMeasureToken, name, startTime, endTime - startTime, detail);
            performanceEntries.push(entry);
            return entry;
        }
        clearMeasures(name) { this.__clear('measure', name); }
        clearResourceTimings() { this.__clear('resource'); }
        setResourceTimingBufferSize(_size) {}
        __clear(type, name) {
            for (let index = performanceEntries.length - 1; index >= 0; index--) {
                const entry = performanceEntries[index];
                if (entry.entryType === type && (name === undefined || entry.name === String(name)))
                    performanceEntries.splice(index, 1);
            }
        }
    }

    Object.defineProperty(PerformanceEntry.prototype, Symbol.toStringTag,
        { value: 'PerformanceEntry', configurable: true });
    Object.defineProperty(PerformanceMark.prototype, Symbol.toStringTag,
        { value: 'PerformanceMark', configurable: true });
    Object.defineProperty(PerformanceMeasure.prototype, Symbol.toStringTag,
        { value: 'PerformanceMeasure', configurable: true });
    Object.defineProperty(Performance.prototype, Symbol.toStringTag,
        { value: 'Performance', configurable: true });
    windowObject.PerformanceEntry = PerformanceEntry;
    windowObject.PerformanceMark = PerformanceMark;
    windowObject.PerformanceMeasure = PerformanceMeasure;
    windowObject.Performance = Performance;
    windowObject.performance = new Performance();
    iframeWindow.performance = windowObject.performance;
