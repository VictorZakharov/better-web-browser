    // Screen Wake Lock: the browser owns the platform request. A sentinel is
    // created only after its document-scoped request is acknowledged.
    let releaseAllWakeLocks = () => {};
    let applyWakeLockUpdate = () => false;
    if (host('wakeLockAvailable')) {
        const NativePromise = Promise, NativeEvent = Event, NativeDOMException = DOMException;
        const NativeTypeError = TypeError, NativeString = String, NativeNumber = Number;
        const apply = Reflect.apply, create = Object.create;
        const promiseResolve = NativePromise.resolve, promiseReject = NativePromise.reject;
        const isSafeInteger = NativeNumber.isSafeInteger;
        const mapSize = Object.getOwnPropertyDescriptor(Map.prototype, 'size').get;
        const resolved = () => apply(promiseResolve, NativePromise, []);
        const rejected = error => apply(promiseReject, NativePromise, [error]);
        const size = map => apply(mapSize, map, []);
        const dispatchEvent = EventTarget.prototype.dispatchEvent;
        const mapGet = Map.prototype.get, mapSet = Map.prototype.set;
        const mapDelete = Map.prototype.delete, mapKeys = Map.prototype.keys;
        const weakGet = WeakMap.prototype.get, weakSet = WeakMap.prototype.set;
        const weakHas = WeakMap.prototype.has;
        const get = (map, key) => apply(mapGet, map, [key]);
        const set = (map, key, value) => apply(mapSet, map, [key, value]);
        const remove = (map, key) => apply(mapDelete, map, [key]);
        const slot = value => apply(weakGet, slots, [value]);
        const hasSlot = value => apply(weakHas, slots, [value]);
        const slots = new WeakMap(), pending = new Map(), active = new Map();
        let nextId = 1;
        const denied = () => new NativeDOMException('Screen wake lock is not available', 'NotAllowedError');
        const release = (id, notifyBrowser) => {
            const sentinel = get(active, id);
            if (!sentinel) return;
            if (notifyBrowser) host('wakeLockRequest', id, false);
            remove(active, id);
            slot(sentinel).released = true;
            apply(dispatchEvent, sentinel, [markTrusted(new NativeEvent('release'))]);
        };
        releaseAllWakeLocks = () => {
            for (const id of apply(mapKeys, active, [])) {
                try { release(id, true); }
                catch (_) { release(id, false); } // Browser visibility retirement remains authoritative.
            }
        };
        function WakeLockSentinel() { throw new NativeTypeError('Illegal constructor'); }
        WakeLockSentinel.prototype = create(EventTarget.prototype, {
            constructor: {value: WakeLockSentinel, writable: true, configurable: true},
            type: {get() { if (!hasSlot(this)) throw new NativeTypeError('Invalid WakeLockSentinel'); return 'screen'; }},
            released: {get() { const state = slot(this); if (!state) throw new NativeTypeError('Invalid WakeLockSentinel'); return state.released; }},
            release: {value: function release() {
                const state = slot(this);
                if (!state) return rejected(new NativeTypeError('Invalid WakeLockSentinel'));
                if (!state.released) {
                    try { release(state.id, true); }
                    catch (error) { return rejected(error); }
                }
                return resolved();
            }, writable: true, configurable: true},
            [Symbol.toStringTag]: {value: 'WakeLockSentinel', configurable: true}
        });
        defineEventHandler(WakeLockSentinel.prototype, null, 'release');
        const wakeLock = create(WakeLock.prototype);
        function WakeLock() { throw new NativeTypeError('Illegal constructor'); }
        Object.defineProperties(WakeLock.prototype, {
            request: {value: function request(type = 'screen') {
                if (this !== wakeLock) return rejected(new NativeTypeError('Invalid WakeLock'));
                if (typeof type === 'symbol' || NativeString(type) !== 'screen')
                    return rejected(new NativeTypeError('Unsupported wake lock type'));
                if (document.hidden || size(pending) + size(active) >= 16) return rejected(denied());
                const id = nextId++;
                if (!isSafeInteger(id)) return rejected(denied());
                return new NativePromise((resolve, reject) => {
                    set(pending, id, {resolve, reject});
                    try { host('wakeLockRequest', id, true); }
                    catch (error) { remove(pending, id); reject(error); }
                });
            }, writable: true, configurable: true},
            [Symbol.toStringTag]: {value: 'WakeLock', configurable: true}
        });
        Object.defineProperty(windowObject.navigator, 'wakeLock', {
            configurable: true, enumerable: true, get: () => wakeLock
        });
        Object.assign(windowObject, {WakeLock, WakeLockSentinel});
        applyWakeLockUpdate = input => {
            const id = NativeNumber(input.requestId);
            const waiting = get(pending, id);
            if (waiting) {
                remove(pending, id);
                if (input.disposition !== 'granted' || document.hidden) {
                    if (input.disposition === 'granted') {
                        try { host('wakeLockRequest', id, false); } catch (_) {}
                    }
                    waiting.reject(denied());
                    return true;
                }
                const sentinel = create(WakeLockSentinel.prototype);
                apply(weakSet, slots, [sentinel, {id, released: false}]);
                set(active, id, sentinel);
                waiting.resolve(sentinel);
                return true;
            }
            if (input.disposition === 'released') release(id, false);
            return true;
        };
    }
