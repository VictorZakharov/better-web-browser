// Geolocation callbacks are fulfilled only by the browser's origin-bound OS broker.
// https://www.w3.org/TR/geolocation/#geolocation_interface
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('geolocationAvailable')) return;

    const pending = new Map();
    const internal = {};
    const coordinateData = new WeakMap();
    const positionData = new WeakMap();
    const errorData = new WeakMap();
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
        constructor(key) {
            if (key !== internal) throw new TypeError('Illegal constructor');
        }
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
    class GeolocationCoordinates {
        constructor(key, data) {
            if (key !== internal) throw new TypeError('Illegal constructor');
            coordinateData.set(this, data);
        }
        get latitude() { return coordinateData.get(this).latitude; }
        get longitude() { return coordinateData.get(this).longitude; }
        get accuracy() { return coordinateData.get(this).accuracy; }
        get altitude() { return coordinateData.get(this).altitude; }
        get altitudeAccuracy() { return coordinateData.get(this).altitudeAccuracy; }
        get heading() { return coordinateData.get(this).heading; }
        get speed() { return coordinateData.get(this).speed; }
        toJSON() {
            return { latitude: this.latitude, longitude: this.longitude,
                accuracy: this.accuracy, altitude: this.altitude,
                altitudeAccuracy: this.altitudeAccuracy, heading: this.heading,
                speed: this.speed };
        }
    }
    class GeolocationPosition {
        constructor(key, data) {
            if (key !== internal) throw new TypeError('Illegal constructor');
            positionData.set(this, {
                coords: new GeolocationCoordinates(internal, data.coords),
                timestamp: data.timestamp
            });
        }
        get coords() { return positionData.get(this).coords; }
        get timestamp() { return positionData.get(this).timestamp; }
        toJSON() { return { coords: this.coords.toJSON(), timestamp: this.timestamp }; }
    }
    class GeolocationPositionError {
        constructor(key, code, message) {
            if (key !== internal) throw new TypeError('Illegal constructor');
            errorData.set(this, { code, message });
        }
        get code() { return errorData.get(this).code; }
        get message() { return errorData.get(this).message; }
    }
    const geolocation = new Geolocation(internal);
    Object.defineProperty(navigator, 'geolocation', {
        configurable: true, enumerable: true, get: () => geolocation
    });
    globalThis.Geolocation = Geolocation;
    globalThis.GeolocationCoordinates = GeolocationCoordinates;
    globalThis.GeolocationPosition = GeolocationPosition;
    globalThis.GeolocationPositionError = GeolocationPositionError;
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
            request.success(new GeolocationPosition(internal, update.position));
        } else if (request.error) {
            request.error(new GeolocationPositionError(internal, update.code, update.message));
        }
    };
})();
