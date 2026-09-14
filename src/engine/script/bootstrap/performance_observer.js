    // Performance Timeline §5–6. User Timing retains entries until explicitly cleared.
    // Resource/Navigation/Paint/Long Task types are not advertised without real producers.
    const supportedTypes = Object.freeze(['mark', 'measure']);
    const observerState = new WeakMap(), listState = new WeakMap(), registered = new Set();
    let taskQueued = false;
    function queueObserverTask() {
        if (taskQueued) return;
        taskQueued = true;
        hooks.queue(() => {
            taskQueued = false;
            for (const observer of [...registered]) {
                const state = observerState.get(observer);
                const buffer = state.buffer.splice(0);
                if (!buffer.length) continue;
                const list = Object.create(PerformanceObserverEntryList.prototype);
                listState.set(list, buffer);
                const options = state.droppedRequired ? {droppedEntriesCount: 0} : {};
                state.droppedRequired = false;
                try { state.callback.call(observer, list, observer, options); }
                catch (error) { hooks.report(error); }
            }
        });
    }
    function publishEntry(entry) {
        entries.push(entry);
        const type = entryState.get(entry).entryType;
        let interested = false;
        for (const observer of registered) {
            const state = observerState.get(observer);
            if (state.types.has(type)) { state.buffer.push(entry); interested = true; }
        }
        // No observable work exists without interested observers; avoid an empty native task.
        if (interested) queueObserverTask();
    }
    class PerformanceObserverEntryList {
        constructor() { throw new TypeError('Illegal constructor'); }
        getEntries() { return filterEntries(brand(listState, this)); }
        getEntriesByType(type) {
            const buffer = brand(listState, this); requireArgument(arguments.length);
            return filterEntries(buffer, undefined, domString(type));
        }
        getEntriesByName(name, type = undefined) {
            const buffer = brand(listState, this); requireArgument(arguments.length);
            name = domString(name); type = type === undefined ? undefined : domString(type);
            return filterEntries(buffer, name, type);
        }
    }
    class PerformanceObserver {
        constructor(callback) {
            if (typeof callback !== 'function') throw new TypeError('PerformanceObserver requires a callback');
            observerState.set(this, {callback, buffer: [], types: new Set(), mode: undefined, droppedRequired: false});
        }
        static get supportedEntryTypes() { return supportedTypes; }
        observe(input = {}) {
            const state = brand(observerState, this), options = dictionary(input);
            const bufferedValue = options.buffered;
            const buffered = bufferedValue === undefined ? undefined : !!bufferedValue;
            let types = options.entryTypes;
            if (types !== undefined) {
                if (types === null || (typeof types !== 'object' && typeof types !== 'function'))
                    throw new TypeError('entryTypes must be a sequence');
                const iterable = types, iterator = iterable[Symbol.iterator];
                if (typeof iterator !== 'function') throw new TypeError('entryTypes must be a sequence');
                types = Array.from({[Symbol.iterator]: () => iterator.call(iterable)}, domString);
            }
            let type = options.type;
            if (type !== undefined) type = domString(type);
            if ((types === undefined && type === undefined) ||
                (types !== undefined && (type !== undefined || buffered !== undefined)))
                throw new TypeError('Use entryTypes alone, or type with optional buffered');
            const mode = types === undefined ? 'single' : 'multiple';
            if (state.mode !== undefined && state.mode !== mode)
                throw new DOMException('Cannot mix single and multiple observer modes', 'InvalidModificationError');
            state.mode = mode; state.droppedRequired = true;
            if (mode === 'multiple') {
                types = types.filter(type => supportedTypes.includes(type));
                if (!types.length) return;
                state.types = new Set(types);
            } else {
                if (!supportedTypes.includes(type)) return;
                state.types.add(type);
            }
            registered.add(this);
            if (mode === 'single' && buffered) {
                state.buffer.push(...entries.filter(entry => entryState.get(entry).entryType === type));
                queueObserverTask();
            }
        }
        disconnect() {
            const state = brand(observerState, this);
            registered.delete(this); state.types.clear(); state.buffer.length = 0;
            // The single/multiple mode intentionally survives disconnect().
        }
        takeRecords() { return brand(observerState, this).buffer.splice(0); }
    }
    for (const ctor of [Performance, PerformanceEntry, PerformanceMark, PerformanceMeasure,
        PerformanceObserver, PerformanceObserverEntryList]) {
        Object.defineProperty(ctor.prototype, Symbol.toStringTag, {value: ctor.name, configurable: true});
        for (const name of Object.getOwnPropertyNames(ctor.prototype)) {
            if (name !== 'constructor') Object.defineProperty(ctor.prototype, name,
                {...Object.getOwnPropertyDescriptor(ctor.prototype, name), enumerable: true});
        }
    }
    Object.defineProperty(PerformanceObserver, 'supportedEntryTypes',
        {...Object.getOwnPropertyDescriptor(PerformanceObserver, 'supportedEntryTypes'), enumerable: true});
    Object.assign(globalThis, {Performance, PerformanceEntry, PerformanceMark, PerformanceMeasure,
        PerformanceObserver, PerformanceObserverEntryList});
    globalThis.performance = new Performance(token);
    hooks.install?.(globalThis.performance);
})();
