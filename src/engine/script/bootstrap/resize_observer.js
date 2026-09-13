    // https://drafts.csswg.org/resize-observer/#algorithms
    // Native code calls gather -> one broadcast -> microtask checkpoint -> gather at increasing
    // flattened-tree depths. No author timer or boundingClientRect participates in delivery.
    const resizeStates = new WeakMap(), resizeObservers = new Set();
    let resizeOrder = 0, resizeRound = [], resizeIndex = 0, resizeSkipped = false;
    const resizeTarget = target => {
        if (!(target instanceof Element)) throw new TypeError('ResizeObserver target must be an Element');
    };
    const observedSize = (box, option) => option === 'border-box' ? [box[4], box[5]]
        : option === 'device-pixel-content-box' ? [Math.round(box[2] * box[6]), Math.round(box[3] * box[6])]
        : [box[2], box[3]];
    class ResizeObserver {
        constructor(callback) {
            if (typeof callback !== 'function') throw new TypeError('ResizeObserver requires a callback');
            resizeStates.set(this, {callback, order: ++resizeOrder, targets: new Map(), active: []});
        }
        observe(target, options = {}) {
            const state = branded(resizeStates, this);
            resizeTarget(target);
            if (options != null && typeof options !== 'object' && typeof options !== 'function')
                throw new TypeError('ResizeObserverOptions must be a dictionary');
            let box = options?.box;
            box = box === undefined ? 'content-box' : `${box}`;
            if (!['content-box', 'border-box', 'device-pixel-content-box'].includes(box))
                throw new TypeError('Invalid ResizeObserver box');
            // Re-observing replaces the observation and appends it in registration order.
            state.targets.delete(target);
            state.targets.set(target, {box, last: [-1, -1]});
            resizeObservers.add(this);
            host('resizeObserverSchedule');
        }
        unobserve(target) {
            const state = branded(resizeStates, this);
            resizeTarget(target);
            state.targets.delete(target);
            if (!state.targets.size) resizeObservers.delete(this);
        }
        disconnect() {
            const state = branded(resizeStates, this);
            state.targets.clear();
            state.active = [];
            resizeObservers.delete(this);
        }
    }
    for (const name of ['observe', 'unobserve', 'disconnect'])
        Object.defineProperty(ResizeObserver.prototype, name,
            {...Object.getOwnPropertyDescriptor(ResizeObserver.prototype, name), enumerable: true});
    Object.defineProperty(ResizeObserver.prototype, Symbol.toStringTag, {value: 'ResizeObserver', configurable: true});
    windowObject.ResizeObserver = ResizeObserver;
    windowObject.__gatherResizeObservers = depth => {
        resizeRound = [...resizeObservers].sort((a, b) => resizeStates.get(a).order - resizeStates.get(b).order);
        resizeIndex = 0;
        resizeSkipped = false;
        let active = false;
        for (const observer of resizeRound) {
            const state = resizeStates.get(observer);
            state.active = [];
            for (const [target, observation] of state.targets) {
                const box = host('resizeObservation', target.__id);
                const size = observedSize(box, observation.box);
                if (size[0] === observation.last[0] && size[1] === observation.last[1]) continue;
                if (box[7] > depth) { state.active.push([target, observation]); active = true; }
                else resizeSkipped = true;
            }
        }
        return active;
    };
    windowObject.__broadcastResizeObserver = () => {
        while (resizeIndex < resizeRound.length) {
            const observer = resizeRound[resizeIndex++], state = resizeStates.get(observer);
            if (!state.active.length) continue;
            let shallowest = Infinity;
            const entries = [];
            for (const [target, observation] of state.active) {
                const box = host('resizeObservation', target.__id);
                entries.push(resizeEntry(target, box));
                observation.last = observedSize(box, observation.box);
                shallowest = Math.min(shallowest, box[7]);
            }
            state.active = [];
            try { state.callback.call(observer, entries, observer); }
            catch (error) { reportGlobalException(error, 'ResizeObserver'); }
            return shallowest;
        }
        return -1;
    };
    windowObject.__finishResizeObservers = () => {
        resizeRound = [];
        if (resizeSkipped) {
            host('resizeObserverSchedule');
            const message = 'ResizeObserver loop completed with undelivered notifications.';
            const event = markTrusted(new ErrorEvent('error', {message, cancelable: true}));
            if (windowObject.dispatchEvent(event)) console.error(message);
        }
    };
