// Geolocation callbacks are fulfilled only by the browser's origin-bound OS broker.
// https://www.w3.org/TR/geolocation/#geolocation_interface
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('geolocationAvailable')) return;

    const pending = new Map();
    function duration(value, fallback) {
        if (value === undefined) return fallback;
        const number = Number(value);
        if (Number.isNaN(number) || number <= 0) return 0;
        if (!Number.isFinite(number)) return null; // Infinity on the wire
        return Math.min(Math.trunc(number), Number.MAX_SAFE_INTEGER);
    }
    function optionsSnapshot(options) {
        if (options == null) options = {};
        if (typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('Geolocation options must be a dictionary');
        return {
            highAccuracy: !!options.enableHighAccuracy,
            timeout: duration(options.timeout, null),
            maximumAge: duration(options.maximumAge, 0)
        };
    }
    function start(watch, success, error, options) {
        if (typeof success !== 'function')
            throw new TypeError('Geolocation success callback must be a function');
        if (error !== undefined && error !== null && typeof error !== 'function')
            throw new TypeError('Geolocation error callback must be a function');
        const id = Number(native('geolocationRequest', JSON.stringify({
            kind: 'start', watch, ...optionsSnapshot(options)
        })));
        pending.set(id, { watch, success, error });
        return id;
    }
    class Geolocation {
        getCurrentPosition(success, error, options) {
            start(false, success, error, options);
        }
        watchPosition(success, error, options) {
            return start(true, success, error, options);
        }
        clearWatch(id) {
            id = Number(id);
            const request = pending.get(id);
            if (!request || !request.watch) return;
            pending.delete(id);
            native('geolocationRequest', JSON.stringify({ kind: 'clear', id }));
        }
    }
    const geolocation = new Geolocation();
    Object.defineProperty(navigator, 'geolocation', {
        configurable: true, enumerable: true, get: () => geolocation
    });
    globalThis.Geolocation = Geolocation;
    globalThis.GeolocationPositionError = class GeolocationPositionError {
        constructor(code, message) {
            this.code = code;
            this.message = message;
        }
    };
    Object.assign(GeolocationPositionError, {
        PERMISSION_DENIED: 1, POSITION_UNAVAILABLE: 2, TIMEOUT: 3
    });
    Object.assign(GeolocationPositionError.prototype, {
        PERMISSION_DENIED: 1, POSITION_UNAVAILABLE: 2, TIMEOUT: 3
    });
    globalThis.__receiveGeolocationUpdate = payload => {
        const update = JSON.parse(String(payload));
        const request = pending.get(Number(update.id));
        if (!request) return;
        if (update.terminal) pending.delete(Number(update.id));
        if (update.position) {
            const coords = Object.freeze(update.position.coords);
            request.success(Object.freeze({ coords, timestamp: update.position.timestamp }));
        } else if (request.error) {
            request.error(Object.freeze(new GeolocationPositionError(update.code, update.message)));
        }
    };
})();
