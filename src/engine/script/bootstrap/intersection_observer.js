(() => {
    'use strict';
    const states = new WeakMap(), entries = new WeakMap();
    const observers = new Set(), queued = new Set();
    let nextOrder = 0, delivery = [], deliveryIndex = 0;
    const rectangle = values => new DOMRectReadOnly(...values);
    const zero = () => new DOMRectReadOnly();
    const targetCheck = target => {
        if (!(target instanceof Element)) throw new TypeError('Observation target must be an Element');
    };
    const drain = observer => {
        const state = branded(states, observer), records = state.records;
        state.records = [];
        queued.delete(observer);
        return records;
    };
    const dictionary = value => {
        if (value != null && typeof value !== 'object' && typeof value !== 'function')
            throw new TypeError('Options must be a dictionary');
        return value ?? {};
    };
    const parseMargin = value => {
        const tokens = `${value === undefined ? '0px' : value}`.trim().split(/\s+/).filter(Boolean);
        if (tokens.length > 4) throw new DOMException('Invalid intersection margin', 'SyntaxError');
        const units = {px: 1, cm: 96 / 2.54, mm: 96 / 25.4, q: 96 / 101.6, in: 96, pt: 96 / 72, pc: 16};
        const parsed = tokens.map(token => {
            const match = token.match(/^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)(px|cm|mm|q|in|pt|pc|%)$/i);
            if (!match || !Number.isFinite(+match[1]))
                throw new DOMException('Invalid intersection margin', 'SyntaxError');
            const unit = match[2].toLowerCase();
            return {value: unit === '%' ? +match[1] : Math.floor(+match[1] * units[unit]),
                unit: unit === '%' ? '%' : 'px'};
        });
        if (!parsed.length) parsed.push({value: 0, unit: 'px'});
        if (parsed.length === 1) parsed.push(parsed[0], parsed[0], parsed[0]);
        else if (parsed.length === 2) parsed.push(parsed[0], parsed[1]);
        else if (parsed.length === 3) parsed.push(parsed[1]);
        return parsed;
    };
    const serialize = values => values.map(v => `${v.value}${v.unit}`).join(' ');
    const expand = (rect, margins) => {
        const [t, r, b, l] = margins.map(m => m.unit === '%' ? m.value * rect.width / 100 : m.value);
        // Keep inverted edges for over-contracted margins; DOMRect normalizes negative sizes.
        return {left: rect.left - l, top: rect.top - t, right: rect.right + r, bottom: rect.bottom + b,
            width: rect.width + l + r, height: rect.height + t + b};
    };
    const intersect = (a, b, x = true, y = true) => {
        if (!a) return null;
        const left = x ? Math.max(a.left, b.left) : a.left;
        const right = x ? Math.min(a.right, b.right) : a.right;
        const top = y ? Math.max(a.top, b.top) : a.top;
        const bottom = y ? Math.min(a.bottom, b.bottom) : a.bottom;
        return right < left || bottom < top ? null : new DOMRectReadOnly(left, top, right - left, bottom - top);
    };
    class IntersectionObserverEntry {
        constructor(init) {
            init = dictionary(init);
            for (const key of ['time', 'rootBounds', 'boundingClientRect', 'intersectionRect',
                'isIntersecting', 'intersectionRatio', 'target'])
                if (!(key in init)) throw new TypeError(`Missing ${key}`);
            targetCheck(init.target);
            entries.set(this, {...init});
        }
    }
    for (const key of ['time', 'rootBounds', 'boundingClientRect', 'intersectionRect',
        'isIntersecting', 'intersectionRatio', 'target', 'isVisible'])
        Object.defineProperty(IntersectionObserverEntry.prototype, key, {
            configurable: true, enumerable: true, get() { return branded(entries, this)[key]; }
        });
    class IntersectionObserver {
        constructor(callback, options = {}) {
            if (typeof callback !== 'function') throw new TypeError('IntersectionObserver requires a callback');
            options = dictionary(options);
            const root = options.root ?? null;
            if (root !== null && !(root instanceof Element) && !(root instanceof Document))
                throw new TypeError('Root must be an Element, Document, or null');
            const rootMargin = parseMargin(options.rootMargin), scrollMargin = parseMargin(options.scrollMargin);
            const threshold = options.threshold === undefined ? 0 : options.threshold;
            const thresholds = threshold != null && typeof threshold === 'object' && threshold[Symbol.iterator]
                ? Array.from(threshold, value => +value) : [+threshold];
            if (thresholds.some(value => !Number.isFinite(value))) throw new TypeError('Threshold must be finite');
            if (thresholds.some(value => value < 0 || value > 1)) throw new RangeError('Threshold must be in [0, 1]');
            if (!thresholds.length) thresholds.push(0);
            const trackVisibility = !!options.trackVisibility;
            const delay = Math.max(trackVisibility ? 100 : 0, Number(options.delay) | 0);
            states.set(this, {callback, root, rootMargin, scrollMargin, thresholds: Object.freeze(thresholds.sort((a,b) => a-b)),
                trackVisibility, delay, targets: new Map(), records: [], order: ++nextOrder, wakeup: false});
        }
        observe(target) {
            const state = branded(states, this);
            targetCheck(target);
            if (state.targets.has(target)) return;
            state.targets.set(target, {time: -Infinity, threshold: -1, intersecting: false});
            observers.add(this);
            host('intersectionObserverSchedule');
        }
        unobserve(target) {
            const state = branded(states, this);
            targetCheck(target);
            state.targets.delete(target);
            if (!state.targets.size) observers.delete(this);
        }
        disconnect() {
            const state = branded(states, this);
            state.targets.clear();
            observers.delete(this);
            // Disconnect stops future observations, not entries already queued for delivery.
        }
        takeRecords() {
            return drain(this);
        }
    }
    for (const key of ['root', 'rootMargin', 'scrollMargin', 'thresholds', 'delay', 'trackVisibility'])
        Object.defineProperty(IntersectionObserver.prototype, key, {configurable: true, enumerable: true,
            get() { const state = branded(states, this); return key.endsWith('Margin') ? serialize(state[key]) : state[key]; }
        });
    for (const type of [IntersectionObserver, IntersectionObserverEntry])
        Object.defineProperty(type.prototype, Symbol.toStringTag, {value: type.name, configurable: true});
    for (const name of ['observe', 'unobserve', 'disconnect', 'takeRecords'])
        Object.defineProperty(IntersectionObserver.prototype, name,
            {...Object.getOwnPropertyDescriptor(IntersectionObserver.prototype, name), enumerable: true});

    const gather = () => {
        const time = host('performanceNow');
        for (const observer of observers) {
            const state = states.get(observer);
            for (const [target, previous] of state.targets) {
                const remaining = state.delay - (time - previous.time);
                if (remaining > 0) {
                    if (!state.wakeup) {
                        state.wakeup = true;
                        queueTimer(() => { state.wakeup = false; if (state.targets.size)
                            host('intersectionObserverSchedule'); }, remaining, false, [],
                            'intersection observation delay', 'intersectionObserverDelay');
                    }
                    continue;
                }
                previous.time = time;
                const [valid, targetBox, rootBox, rootScroll, clips] =
                    host('intersectionGeometry', target.__id, state.root?.__id ?? 0);
                const boundingClientRect = rectangle(targetBox);
                const rootBounds = expand(rectangle(rootBox), state.rootMargin);
                let root = rootBounds;
                // Root and scroll margins both apply to a scrolling root; percentages use
                // the original undilated rectangle's width, not the previously expanded width.
                if (rootScroll) {
                    const delta = expand(rectangle(rootBox), state.scrollMargin);
                    root = {left: root.left + delta.left - rootBox[0], top: root.top + delta.top - rootBox[1],
                        right: root.right + delta.right - rootBox[0] - rootBox[2],
                        bottom: root.bottom + delta.bottom - rootBox[1] - rootBox[3]};
                }
                let intersection = valid ? boundingClientRect : null;
                for (const [box, x, y, scroll] of clips)
                    intersection = intersect(intersection, scroll ? expand(rectangle(box), state.scrollMargin) : rectangle(box), x, y);
                intersection = intersect(intersection, root);
                const isIntersecting = intersection !== null;
                const area = boundingClientRect.width * boundingClientRect.height;
                const ratio = area ? (intersection ? intersection.width * intersection.height / area : 0) : +isIntersecting;
                const threshold = state.thresholds.filter(value => value <= ratio).length;
                if (previous.threshold === threshold && previous.intersecting === isIntersecting) continue;
                previous.threshold = threshold;
                previous.intersecting = isIntersecting;
                // Without a compositor occlusion proof we cannot assert v2 visibility.
                // https://w3c.github.io/IntersectionObserver/#compute-visibility
                state.records.push(new IntersectionObserverEntry({time, target, boundingClientRect,
                    rootBounds: new DOMRectReadOnly(rootBounds.left, rootBounds.top,
                        Math.max(0, rootBounds.right-rootBounds.left), Math.max(0, rootBounds.bottom-rootBounds.top)),
                    intersectionRect: intersection ?? zero(), isIntersecting, intersectionRatio: ratio, isVisible: false}));
                queued.add(observer);
                host('intersectionObserverTask');
            }
        }
    };
    // Sampling does not invoke author callbacks. The embedding queues a separate task and
    // performs a microtask checkpoint after each callback, like Web IDL callback cleanup.
    Object.assign(windowObject, {IntersectionObserver, IntersectionObserverEntry,
        __gatherIntersectionObservers: gather,
        __beginIntersectionDelivery() {
            delivery = [...queued].sort((a,b) => states.get(a).order - states.get(b).order);
            deliveryIndex = 0;
        },
        __broadcastIntersectionObserver() {
            while (deliveryIndex < delivery.length) {
                const observer = delivery[deliveryIndex++], state = states.get(observer);
                const records = drain(observer);
                if (!records.length) continue;
                try { state.callback.call(observer, records, observer); }
                catch (error) { reportGlobalException(error, 'IntersectionObserver'); }
                return true;
            }
            delivery = [];
            return false;
        }
    });
})();
