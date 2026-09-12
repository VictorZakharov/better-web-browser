    // Callback ownership is independent of setTimeout IDs. Native scheduling supplies the
    // actual idle period; author-overridable clocks/properties never define its remaining time.
    // https://w3c.github.io/requestidlecallback/
    const idleCallbacks = new Map();
    const idleDeadlines = new WeakMap();
    const deadlineState = idleDeadlines.get.bind(idleDeadlines);
    const setDeadlineState = idleDeadlines.set.bind(idleDeadlines);
    let nextIdleCallback = 0;
    class IdleDeadline {
        constructor() { throw new TypeError('Illegal constructor'); }
        get didTimeout() {
            const state = deadlineState(this);
            if (!state) throw new TypeError('Incompatible IdleDeadline receiver');
            return state.timedOut;
        }
        timeRemaining() {
            const state = deadlineState(this);
            if (!state) throw new TypeError('Incompatible IdleDeadline receiver');
            return state.timedOut ? 0 : host('idleTimeRemaining', state.period);
        }
    }
    for (const name of ['didTimeout', 'timeRemaining']) {
        Object.defineProperty(IdleDeadline.prototype, name,
            {...Object.getOwnPropertyDescriptor(IdleDeadline.prototype, name), enumerable: true});
    }
    Object.defineProperty(IdleDeadline.prototype, Symbol.toStringTag,
        {value: 'IdleDeadline', configurable: true});
    windowObject.IdleDeadline = IdleDeadline;
    windowObject.requestIdleCallback = function requestIdleCallback(callback, options = {}) {
        if (this != null && this !== windowObject) throw new TypeError('Illegal invocation');
        if (typeof callback !== 'function') throw new TypeError('requestIdleCallback requires a callback');
        if (options != null && typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('IdleRequestOptions must be a dictionary');
        const timeout = options?.timeout;
        // Web IDL unsigned long conversion (including rejecting BigInt and Symbol).
        const delay = timeout === undefined ? 0 : +timeout >>> 0;
        do { nextIdleCallback = (nextIdleCallback + 1) >>> 0; }
        while (!nextIdleCallback || idleCallbacks.has(nextIdleCallback));
        host('idleSchedule', nextIdleCallback, delay);
        idleCallbacks.set(nextIdleCallback, callback);
        return nextIdleCallback;
    };
    windowObject.cancelIdleCallback = function cancelIdleCallback(handle) {
        if (this != null && this !== windowObject) throw new TypeError('Illegal invocation');
        if (!arguments.length) throw new TypeError('cancelIdleCallback requires a handle');
        handle = +handle >>> 0;
        idleCallbacks.delete(handle);
        host('idleCancel', handle);
    };
    windowObject.__idleLabel = id => 'requestIdleCallback: ' + describeTimerCallback(idleCallbacks.get(id));
    windowObject.__runIdleCallback = (id, period, timedOut) => {
        const callback = idleCallbacks.get(id);
        if (!callback) return;
        idleCallbacks.delete(id);
        const deadline = Object.create(IdleDeadline.prototype);
        setDeadlineState(deadline, {period, timedOut});
        try { callback(deadline); }
        catch (error) { reportGlobalException(error, 'idle callback'); }
    };
