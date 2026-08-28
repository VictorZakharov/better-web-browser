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
    windowObject.requestAnimationFrame = callback => queueTimer(
        () => callback(performance.now()), 16, false, [], 'requestAnimationFrame: ' + describeTimerCallback(callback));
    windowObject.cancelAnimationFrame = windowObject.clearTimeout;
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
            name = String(name).toLowerCase();
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
    windowObject.matchMedia = query => {
        query = String(query);
        return { media: query, matches: host('mediaMatches', query), onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; } };
    };
    windowObject.CSS = { supports() { return false; }, escape(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, match => '\\' + match); } };
    windowObject.Image = class Image extends HTMLImageElement {
        constructor() {
            const element = document.createElement('img');
            Object.defineProperty(element, 'src', {
                configurable: true,
                get() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); },
                set(value) {
                    this.setAttribute('src', String(value));
                    setTimeout(() => this.dispatchEvent(new Event('error')), 0);
                }
            });
            return element;
        }
    };
    const mutationRegistrations = new WeakMap();
    const pendingMutationObservers = new Set();
    let mutationObserverMicrotaskQueued = false;
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
        for (let node = target; node; node = node.parentNode) {
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
            if (!normalized.childList && !normalized.attributes && !normalized.characterData)
                throw new TypeError('MutationObserver options must select at least one mutation type');
            let registrations = mutationRegistrations.get(target);
            if (!registrations) mutationRegistrations.set(target, registrations = new Map());
            registrations.set(this, normalized);
            this.targets.add(target);
        }
        disconnect() {
            for (const target of this.targets) mutationRegistrations.get(target)?.delete(this);
            this.targets.clear();
            this.records.length = 0;
            pendingMutationObservers.delete(this);
        }
        takeRecords() { return this.records.splice(0); }
    };
    windowObject.ResizeObserver = class { constructor(callback) { this.callback = callback; } observe() {} unobserve() {} disconnect() {} };
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
