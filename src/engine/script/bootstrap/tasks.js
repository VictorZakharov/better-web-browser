    const timers = new Map();
    const describeTimerCallback = callback => {
        if (typeof callback !== 'function') return 'string callback';
        const name = String(callback.name || '').trim();
        try {
            const source = Function.prototype.toString.call(callback).replace(/\s+/g, ' ').trim();
            return (name ? name + ': ' + source : source || 'anonymous callback').slice(0, 160);
        }
        catch (_) { return (name || 'anonymous callback').slice(0, 160); }
    };
    const queueTimer = (callback, delay, repeat, args, label = describeTimerCallback(callback),
        scheduleOperation = 'timerSchedule') => {
        const id = nextTimer++;
        delay = Math.max(0, Number(delay) || 0);
        timers.set(id, { callback, repeat, args, label });
        host(scheduleOperation, id, delay, repeat);
        return id;
    };
    windowObject.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    windowObject.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    windowObject.clearTimeout = windowObject.clearInterval = id => {
        id = Number(id);
        timers.delete(id);
        host('timerCancel', id);
    };
    let nextAnimationFrame = 1;
    let animationFrameTimer = null;
    let pendingAnimationFrameCallbacks = new Map();
    let activeAnimationFrameCallbacks = null;
    // A rendering opportunity snapshots every pending callback and invokes that whole batch with
    // one timestamp. A callback requested while the batch is running belongs to the next frame.
    // https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#animation-frames
    const scheduleAnimationFrame = () => {
        if (animationFrameTimer !== null || !pendingAnimationFrameCallbacks.size) return;
        animationFrameTimer = queueTimer(() => {
            animationFrameTimer = null;
            activeAnimationFrameCallbacks = pendingAnimationFrameCallbacks;
            pendingAnimationFrameCallbacks = new Map();
            const timestamp = performance.now();
            for (const [id, callback] of activeAnimationFrameCallbacks) {
                if (!activeAnimationFrameCallbacks.delete(id)) continue;
                try { callback(timestamp); }
                catch (error) { reportGlobalException(error); }
            }
            activeAnimationFrameCallbacks = null;
            scheduleAnimationFrame();
        }, 16, false, [], 'requestAnimationFrame callbacks');
    };
    windowObject.requestAnimationFrame = callback => {
        if (typeof callback !== 'function')
            throw new TypeError('requestAnimationFrame requires a callback');
        const id = nextAnimationFrame++;
        pendingAnimationFrameCallbacks.set(id, callback);
        scheduleAnimationFrame();
        return id;
    };
    windowObject.cancelAnimationFrame = id => {
        id = Number(id);
        pendingAnimationFrameCallbacks.delete(id);
        activeAnimationFrameCallbacks?.delete(id);
        if (!pendingAnimationFrameCallbacks.size && animationFrameTimer !== null) {
            windowObject.clearTimeout(animationFrameTimer);
            animationFrameTimer = null;
        }
    };
    // Idle periods are user-agent defined and capped at 50 ms by the cooperative scheduling
    // specification. Breeze opens one only after a quiet 50 ms task window; an earlier explicit
    // timeout wins the race and receives a zero-length, timed-out deadline.
    // https://w3c.github.io/requestidlecallback/#idle-periods
    const idleCallbackDelay = 50;
    const idleDeadlineToken = {};
    windowObject.IdleDeadline = class IdleDeadline {
        constructor(token, deadline, didTimeout) {
            if (token !== idleDeadlineToken) throw new TypeError('Illegal constructor');
            this.__deadline = deadline;
            this.__didTimeout = didTimeout;
        }
        get didTimeout() { return this.__didTimeout; }
        timeRemaining() { return Math.max(0, this.__deadline - performance.now()); }
    };
    Object.defineProperty(windowObject.IdleDeadline.prototype, Symbol.toStringTag,
        { value: 'IdleDeadline', configurable: true });
    windowObject.requestIdleCallback = (callback, options = {}) => {
        if (typeof callback !== 'function') throw new TypeError('requestIdleCallback requires a callback');
        options = Object(options);
        const convertedTimeout = options.timeout === undefined ? null : Math.max(0, Number(options.timeout) || 0);
        const didTimeout = convertedTimeout !== null && convertedTimeout > 0 && convertedTimeout <= idleCallbackDelay;
        const delay = didTimeout ? convertedTimeout : idleCallbackDelay;
        return queueTimer(() => {
            const deadline = performance.now() + (didTimeout ? 0 : idleCallbackDelay);
            callback(new IdleDeadline(idleDeadlineToken, deadline, didTimeout));
        }, delay, false, [], 'requestIdleCallback: ' + describeTimerCallback(callback), 'idleSchedule');
    };
    windowObject.cancelIdleCallback = windowObject.clearTimeout;
    const reportGlobalException = error => {
        const message = error?.message === undefined ? String(error) : String(error.message);
        const event = markTrusted(new ErrorEvent('error', { cancelable: true, message, error }));
        if (windowObject.dispatchEvent(event)) host('console', 'error', 'Uncaught microtask exception: ' + message);
    };
    windowObject.queueMicrotask = callback => {
        if (typeof callback !== 'function') throw new TypeError('queueMicrotask requires a callback');
        Promise.resolve().then(() => {
            try { callback(); }
            catch (error) { reportGlobalException(error); }
        });
    };
    windowObject.__timerLabel = id => {
        const timer = timers.get(Number(id));
        return timer ? timer.label : 'missing callback';
    };
    windowObject.__runTimer = id => {
        const timer = timers.get(Number(id));
        if (!timer) return false;
        if (!timer.repeat) timers.delete(Number(id));
        if (typeof timer.callback === 'function') timer.callback(...timer.args);
        else (0, eval)(String(timer.callback));
        return true;
    };

    const computedStyleProxy = element => new Proxy({
        getPropertyValue(name) {
            name = String(name);
            if (!name.startsWith('--')) name = name.toLowerCase();
            return element ? host('computedStyle', element.__id, name) : '';
        },
        get cssText() { return ''; }
    }, {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        }
    });
    windowObject.getComputedStyle = element => computedStyleProxy(element);
    windowObject.CSS = {
        supports(property, value) {
            if (arguments.length === 0)
                throw new TypeError("CSS.supports requires at least one argument");
            const condition = arguments.length === 1 ? String(property) :
                '(' + String(property) + ': ' + String(value) + ')';
            return !!host('cssSupports', condition);
        },
        escape(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, match => '\\' + match); }
    };
    const detachedImageLoads = new WeakMap();
    windowObject.Image = class Image extends HTMLImageElement {
        constructor(width, height) {
            const element = document.createElement('img');
            Object.setPrototypeOf(element, new.target.prototype);
            if (width !== undefined) element.setAttribute('width', String(Number(width) >>> 0));
            if (height !== undefined) element.setAttribute('height', String(Number(height) >>> 0));
            Object.defineProperty(element, 'src', {
                configurable: true,
                get() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); },
                set(value) {
                    const source = String(value);
                    this.setAttribute('src', source);
                    const token = (detachedImageLoads.get(this)?.token || 0) + 1;
                    const state = { token, complete: false, promise: null };
                    detachedImageLoads.set(this, state);
                    state.promise = fetch(this.src, {
                        mode: 'no-cors',
                        credentials: 'include',
                        referrerPolicy: 'no-referrer-when-downgrade'
                    }).then(response => {
                        if (!response.ok && response.type !== 'opaque') throw new TypeError('Image request failed');
                        return response.arrayBuffer();
                    }).then(() => {
                        if (detachedImageLoads.get(this)?.token !== token) return;
                        state.complete = true;
                        this.dispatchEvent(new Event('load'));
                    }, () => {
                        if (detachedImageLoads.get(this)?.token !== token) return;
                        state.complete = true;
                        this.dispatchEvent(new Event('error'));
                    });
                }
            });
            Object.defineProperty(element, 'complete', {
                configurable: true,
                get() { return detachedImageLoads.get(this)?.complete ?? true; }
            });
            element.decode = function decode() {
                return detachedImageLoads.get(this)?.promise || Promise.resolve();
            };
            return element;
        }
    };
    const mutationRegistrations = new WeakMap();
    const pendingMutationObservers = new Set();
    const mutationAncestorCache = new WeakMap();
    let mutationRegistrationCount = 0;
    invalidateMutationAncestors = root => {
        const pending = [root];
        while (pending.length) {
            const node = pending.pop();
            mutationAncestorCache.delete(node);
            pending.push(...node.childNodes);
            const shadowRoot = shadowRootForTraversal(node);
            if (shadowRoot) pending.push(shadowRoot);
        }
    };
    const mutationAncestors = target => {
        let ancestors = mutationAncestorCache.get(target);
        if (!ancestors) {
            ancestors = list(host('inclusiveAncestors', target.__id));
            mutationAncestorCache.set(target, ancestors);
        }
        return ancestors;
    };
    let mutationObserverMicrotaskQueued = false;
    const suppressedMutationRecordTargets = new Map();
    const withSuppressedMutationRecords = (target, callback) => {
        const depth = suppressedMutationRecordTargets.get(target) || 0;
        suppressedMutationRecordTargets.set(target, depth + 1);
        try { return callback(); }
        finally {
            if (depth) suppressedMutationRecordTargets.set(target, depth);
            else suppressedMutationRecordTargets.delete(target);
        }
    };
    const queueMutationObserverMicrotask = () => {
        if (mutationObserverMicrotaskQueued) return;
        mutationObserverMicrotaskQueued = true;
        Promise.resolve().then(() => {
            mutationObserverMicrotaskQueued = false;
            const notify = [...pendingMutationObservers];
            pendingMutationObservers.clear();
            for (const observer of notify) {
                const records = observer.takeRecords();
                if (!records.length) continue;
                try { observer.callback.call(observer, records, observer); }
                catch (error) { reportGlobalException(error); }
            }
        });
    };
    const queueMutationRecord = (target, type, details = {}) => {
        if (!mutationRegistrationCount || suppressedMutationRecordTargets.has(target)) return;
        for (const node of mutationAncestors(target)) {
            const registrations = mutationRegistrations.get(node);
            if (!registrations) continue;
            for (const [observer, options] of registrations) {
                if (node !== target && !options.subtree) continue;
                if (type === 'characterData' && !options.characterData) continue;
                if (type === 'attributes' && !options.attributes) continue;
                if (type === 'attributes' && options.attributeFilter &&
                    !options.attributeFilter.includes(details.attributeName)) continue;
                if (type === 'childList' && !options.childList) continue;
                observer.records.push({
                    type,
                    target,
                    addedNodes: details.addedNodes || [],
                    removedNodes: details.removedNodes || [],
                    previousSibling: details.previousSibling || null,
                    nextSibling: details.nextSibling || null,
                    attributeName: details.attributeName || null,
                    attributeNamespace: details.attributeNamespace ?? null,
                    oldValue: (type === 'characterData' && options.characterDataOldValue) ||
                        (type === 'attributes' && options.attributeOldValue) ? details.oldValue ?? null : null
                });
                pendingMutationObservers.add(observer);
            }
        }
        if (pendingMutationObservers.size) queueMutationObserverMicrotask();
    };
    windowObject.MutationObserver = class MutationObserver {
        constructor(callback) {
            if (typeof callback !== 'function') throw new TypeError('MutationObserver requires a callback');
            this.callback = callback;
            this.records = [];
            this.targets = new Set();
        }
        observe(target, options = {}) {
            if (!(target instanceof Node)) throw new TypeError('MutationObserver target must be a Node');
            options = Object(options);
            const normalized = {
                childList: !!options.childList,
                subtree: !!options.subtree,
                attributes: options.attributes === undefined
                    ? options.attributeOldValue !== undefined || options.attributeFilter !== undefined
                    : !!options.attributes,
                attributeOldValue: !!options.attributeOldValue,
                attributeFilter: options.attributeFilter === undefined
                    ? null : Array.from(options.attributeFilter, String),
                characterData: options.characterData === undefined
                    ? options.characterDataOldValue !== undefined
                    : !!options.characterData,
                characterDataOldValue: !!options.characterDataOldValue
            };
            if (options.attributes === false &&
                (options.attributeOldValue !== undefined || options.attributeFilter !== undefined))
                throw new TypeError('Attribute observation options require attributes');
            if (options.characterData === false && options.characterDataOldValue !== undefined)
                throw new TypeError('Character-data old values require characterData');
            if (!normalized.childList && !normalized.attributes && !normalized.characterData)
                throw new TypeError('MutationObserver options must select at least one mutation type');
            let registrations = mutationRegistrations.get(target);
            if (!registrations) mutationRegistrations.set(target, registrations = new Map());
            if (!registrations.has(this)) mutationRegistrationCount++;
            registrations.set(this, normalized);
            this.targets.add(target);
        }
        disconnect() {
            for (const target of this.targets) {
                if (mutationRegistrations.get(target)?.delete(this)) mutationRegistrationCount--;
            }
            this.targets.clear();
            this.records.length = 0;
            pendingMutationObservers.delete(this);
        }
        takeRecords() { return this.records.splice(0); }
    };
    const resizeObservers = new Set();
    let resizeObserverDeliveryPending = false;
    const resizeSize = rect => Object.freeze({ inlineSize: rect.width, blockSize: rect.height });
    const deliverResizeObserverNotifications = () => {
        resizeObserverDeliveryPending = false;
        for (const observer of Array.from(resizeObservers)) {
                const entries = [];
                for (const [target, previous] of observer.targets) {
                    const rect = target.getBoundingClientRect();
                    if (previous && previous.x === rect.x && previous.y === rect.y &&
                        previous.width === rect.width && previous.height === rect.height) continue;
                    observer.targets.set(target, { x: rect.x, y: rect.y, width: rect.width, height: rect.height });
                    const contentRect = Object.freeze({
                        x: 0, y: 0, top: 0, left: 0, right: rect.width, bottom: rect.height,
                        width: rect.width, height: rect.height,
                        toJSON() { return { x: 0, y: 0, top: 0, left: 0, right: rect.width,
                            bottom: rect.height, width: rect.width, height: rect.height }; }
                    });
                    const size = resizeSize(rect);
                    entries.push(Object.freeze({
                        target, contentRect,
                        borderBoxSize: Object.freeze([size]),
                        contentBoxSize: Object.freeze([size]),
                        devicePixelContentBoxSize: Object.freeze([size])
                    }));
                }
                if (entries.length) {
                    try { observer.callback(entries, observer); }
                    catch (error) { reportGlobalException(error); }
                }
        }
    };
    const scheduleResizeObserverDelivery = () => {
        if (resizeObserverDeliveryPending || !resizeObservers.size) return;
        resizeObserverDeliveryPending = true;
        setTimeout(deliverResizeObserverNotifications, 0);
    };
    windowObject.__notifyResizeObservers = deliverResizeObserverNotifications;
    windowObject.ResizeObserver = class ResizeObserver {
        constructor(callback) {
            if (typeof callback !== 'function') throw new TypeError('ResizeObserver requires a callback');
            this.callback = callback;
            this.targets = new Map();
        }
        observe(target) {
            if (!(target instanceof Element)) throw new TypeError('ResizeObserver target must be an Element');
            if (!this.targets.has(target)) this.targets.set(target, null);
            resizeObservers.add(this);
            scheduleResizeObserverDelivery();
        }
        unobserve(target) {
            this.targets.delete(target);
            if (!this.targets.size) resizeObservers.delete(this);
        }
        disconnect() {
            this.targets.clear();
            resizeObservers.delete(this);
        }
    };
    windowObject.crypto = {
        getRandomValues(array) {
            for (let index = 0; index < array.length; index++) array[index] = Math.floor(Math.random() * 256);
            return array;
        },
        randomUUID() {
            return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, character => {
                const value = Math.floor(Math.random() * 16);
                return (character === 'x' ? value : (value & 3) | 8).toString(16);
            });
        }
    };

    windowObject.__wrap = wrap;
    refreshWindowNamedProperties();
    windowObject.__finishDocument = () => {
        document.readyState = 'interactive';
        document.dispatchEvent(markTrusted(new Event('DOMContentLoaded')));
        document.readyState = 'complete';
        windowObject.dispatchEvent(markTrusted(new Event('load')));
    };
})();
