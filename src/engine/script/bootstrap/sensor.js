// Physical sensor readings are delivered only by the browser's permission broker.
// https://www.w3.org/TR/orientation-event/ and https://www.w3.org/TR/generic-sensor/
// https://www.w3.org/TR/magnetometer/ and https://www.w3.org/TR/orientation-sensor/
// https://www.w3.org/TR/ambient-light/
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('sensorAvailable')) {
        delete globalThis.__installSensorListenerHook;
        return;
    }
    const trusted = globalThis.__markTrustedEvent;
    const permissions = new Map();
    const subscriptions = new Map();
    const legacy = {
        deviceorientation: { sensor: 'orientation', listeners: 0, id: 0, retryOnError: false },
        devicemotion: { sensor: 'motion', listeners: 0, id: 0, retryOnError: false }
    };

    function send(command) {
        return Number(native('sensorRequest', JSON.stringify(command)));
    }
    function stopped(subscription) {
        if (!subscription.id) return;
        subscriptions.delete(subscription.id);
        send({ kind: 'stop', sensor: subscription.sensor, id: subscription.id });
        subscription.id = 0;
    }
    function startLegacy(subscription) {
        if (subscription.listeners <= 0 || subscription.id) return;
        const id = send({ kind: 'start', sensor: subscription.sensor });
        subscription.id = id;
        subscriptions.set(id, subscription);
    }
    globalThis.__installSensorListenerHook((target, type, delta) => {
        if (target !== globalThis || !Object.hasOwn(legacy, type)) return;
        const subscription = legacy[type];
        subscription.listeners += delta;
        if (subscription.listeners > 0) startLegacy(subscription);
        else stopped(subscription);
    });
    delete globalThis.__installSensorListenerHook;

    function requestPermission(sensor, absolute) {
        return new Promise((resolve, reject) => {
            try {
                const id = send({ kind: 'requestPermission', sensor, absolute });
                permissions.set(id, resolve);
            } catch (error) { reject(error); }
        });
    }
    const nullableNumber = value => value == null ? null : Number(value);
    const readonly = (target, values) => {
        for (const [name, value] of Object.entries(values))
            Object.defineProperty(target, name, { enumerable: true, value });
    };
    class DeviceOrientationEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            readonly(this, {
                alpha: nullableNumber(init.alpha), beta: nullableNumber(init.beta),
                gamma: nullableNumber(init.gamma), absolute: !!init.absolute
            });
        }
        static requestPermission(absolute = false) {
            return requestPermission('orientation', !!absolute);
        }
    }
    const accelerationToken = {};
    class DeviceMotionEventAcceleration {
        constructor(token, value) {
            if (token !== accelerationToken) throw new TypeError('Illegal constructor');
            readonly(this, {
                x: nullableNumber(value?.x), y: nullableNumber(value?.y), z: nullableNumber(value?.z)
            });
        }
    }
    const rotationToken = {};
    class DeviceMotionEventRotationRate {
        constructor(token, value) {
            if (token !== rotationToken) throw new TypeError('Illegal constructor');
            readonly(this, {
                alpha: nullableNumber(value?.alpha), beta: nullableNumber(value?.beta),
                gamma: nullableNumber(value?.gamma)
            });
        }
    }
    const acceleration = value => value == null ? null :
        new DeviceMotionEventAcceleration(accelerationToken, value);
    const rotationRate = value => value == null ? null :
        new DeviceMotionEventRotationRate(rotationToken, value);
    class DeviceMotionEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            readonly(this, {
                acceleration: acceleration(init.acceleration),
                accelerationIncludingGravity: acceleration(init.accelerationIncludingGravity),
                rotationRate: rotationRate(init.rotationRate),
                interval: Number(init.interval ?? 0)
            });
        }
        static requestPermission() { return requestPermission('motion', false); }
    }

    class SensorErrorEvent extends Event {
        constructor(type, init) {
            if (!init || !(init.error instanceof DOMException))
                throw new TypeError('SensorErrorEvent requires a DOMException error');
            super(type, init);
            readonly(this, { error: init.error });
        }
    }
    const sensorStates = new WeakMap();
    const sensorConstructorToken = {};
    const stateFor = sensor => {
        const state = sensorStates.get(sensor);
        if (!state) throw new TypeError('Invalid Sensor receiver');
        return state;
    };
    function emit(target, event) {
        const handler = target['on' + event.type];
        if (typeof handler !== 'function') {
            target.dispatchEvent(trusted(event));
            return;
        }
        // EventTarget contains and reports listener exceptions. Registering last also
        // preserves the event-handler slot's order after ordinary sensor listeners.
        const invoke = dispatched => handler.call(target, dispatched);
        target.addEventListener(event.type, invoke, { once: true });
        try { target.dispatchEvent(trusted(event)); }
        finally { target.removeEventListener(event.type, invoke); }
    }
    class Sensor extends EventTarget {
        constructor(kind, options = {}, token) {
            super();
            if (token !== sensorConstructorToken) throw new TypeError('Illegal constructor');
            if (options == null || typeof options !== 'object')
                throw new TypeError('Sensor options must be an object');
            const frequency = options.frequency === undefined ? undefined : Number(options.frequency);
            if (frequency !== undefined && (!Number.isFinite(frequency) || frequency <= 0))
                throw new TypeError('Sensor frequency must be a positive finite number');
            sensorStates.set(this, { kind, frequency: frequency === undefined ? undefined : Math.min(frequency, 60),
                phase: 'idle', id: 0, timestamp: null, reading: null });
            this.onactivate = null;
            this.onreading = null;
            this.onerror = null;
        }
        get activated() { return stateFor(this).phase === 'activated'; }
        get hasReading() { return stateFor(this).reading !== null; }
        get timestamp() { return stateFor(this).timestamp; }
        start() {
            const state = stateFor(this);
            if (state.phase !== 'idle') return;
            state.phase = 'activating';
            try {
                const id = send({ kind: 'start', sensor: state.kind, frequencyHz: state.frequency });
                state.id = id;
                subscriptions.set(id, this);
            } catch (error) {
                state.phase = 'idle';
                throw error;
            }
        }
        stop() {
            const state = stateFor(this);
            if (state.phase === 'idle') return;
            stopped({ sensor: state.kind, id: state.id });
            state.id = 0;
            state.phase = 'idle';
            state.timestamp = null;
            state.reading = null;
        }
    }
    class Accelerometer extends Sensor {
        constructor(options = {}) { super('accelerometer', options, sensorConstructorToken); }
        get x() { return stateFor(this).reading?.x ?? null; }
        get y() { return stateFor(this).reading?.y ?? null; }
        get z() { return stateFor(this).reading?.z ?? null; }
    }
    class Gyroscope extends Sensor {
        constructor(options = {}) { super('gyroscope', options, sensorConstructorToken); }
        get x() { return stateFor(this).reading?.x ?? null; }
        get y() { return stateFor(this).reading?.y ?? null; }
        get z() { return stateFor(this).reading?.z ?? null; }
    }
    class Magnetometer extends Sensor {
        constructor(options = {}) { super('magnetometer', options, sensorConstructorToken); }
        get x() { return stateFor(this).reading?.x ?? null; }
        get y() { return stateFor(this).reading?.y ?? null; }
        get z() { return stateFor(this).reading?.z ?? null; }
    }
    class AmbientLightSensor extends Sensor {
        constructor(options = {}) { super('ambientLight', options, sensorConstructorToken); }
        get illuminance() { return stateFor(this).reading?.illuminance ?? null; }
    }
    const orientationConstructorToken = {};
    class OrientationSensor extends Sensor {
        constructor(kind, options = {}, token) {
            if (token !== orientationConstructorToken) throw new TypeError('Illegal constructor');
            if (options != null && typeof options === 'object' &&
                options.referenceFrame !== undefined && options.referenceFrame !== 'device')
                throw new DOMException('Only the device reference frame is supported', 'NotSupportedError');
            super(kind, options, sensorConstructorToken);
        }
        get quaternion() { return stateFor(this).reading?.quaternion ?? null; }
        populateMatrix(targetMatrix) {
            const isArray = targetMatrix instanceof Float32Array || targetMatrix instanceof Float64Array;
            const isDOMMatrix = typeof DOMMatrix !== 'undefined' && targetMatrix instanceof DOMMatrix;
            if (!isArray && !isDOMMatrix)
                throw new TypeError('populateMatrix requires Float32Array, Float64Array, or DOMMatrix');
            if (isArray && targetMatrix.length < 16)
                throw new TypeError('populateMatrix requires at least sixteen elements');
            const quaternion = this.quaternion;
            if (quaternion === null)
                throw new DOMException('No orientation reading is available', 'NotReadableError');
            const [x, y, z, w] = quaternion;
            // W3C Orientation Sensor conversion, in column-major DOMMatrix order.
            const matrix = [
                1 - 2 * y * y - 2 * z * z, 2 * x * y - 2 * z * w,
                2 * x * z + 2 * y * w, 0,
                2 * x * y + 2 * z * w, 1 - 2 * x * x - 2 * z * z,
                2 * y * z - 2 * x * w, 0,
                2 * x * z - 2 * y * w, 2 * y * z + 2 * x * w,
                1 - 2 * x * x - 2 * y * y, 0,
                0, 0, 0, 1
            ];
            if (isArray) targetMatrix.set(matrix);
            else for (let index = 0; index < 16; index++)
                targetMatrix[`m${Math.floor(index / 4) + 1}${index % 4 + 1}`] = matrix[index];
        }
    }
    class AbsoluteOrientationSensor extends OrientationSensor {
        constructor(options = {}) { super('absoluteOrientation', options, orientationConstructorToken); }
    }
    class RelativeOrientationSensor extends OrientationSensor {
        constructor(options = {}) { super('relativeOrientation', options, orientationConstructorToken); }
    }

    globalThis.__receiveSensorUpdate = () => {
        const payload = native('sensorTakeUpdate');
        if (!payload) return;
        const { id, event } = JSON.parse(String(payload));
        if (event.kind === 'permission') {
            const resolve = permissions.get(id);
            permissions.delete(id);
            if (resolve) resolve(event.permission);
            if (event.permission === 'granted') {
                for (const subscription of Object.values(legacy)) {
                    // A prior Start may still be waiting for its denial when permission
                    // arrives. Retry that stream once after the stale denial.
                    subscription.retryOnError = !!subscription.id;
                    startLegacy(subscription);
                }
            }
            return;
        }
        const subscription = subscriptions.get(id);
        if (!subscription) return;
        if (subscription === legacy.deviceorientation || subscription === legacy.devicemotion) {
            if (event.kind === 'error') {
                subscriptions.delete(id);
                subscription.id = 0;
                if (subscription.retryOnError) {
                    subscription.retryOnError = false;
                    startLegacy(subscription);
                }
            } else if (event.kind === 'orientation') {
                window.dispatchEvent(trusted(new DeviceOrientationEvent('deviceorientation', event)));
            } else if (event.kind === 'motion') {
                const vector = values => values == null ? null :
                    ({ x: values[0], y: values[1], z: values[2] });
                const rates = values => values == null ? null :
                    ({ alpha: values[0], beta: values[1], gamma: values[2] });
                window.dispatchEvent(trusted(new DeviceMotionEvent('devicemotion', {
                    acceleration: vector(event.acceleration),
                    accelerationIncludingGravity: vector(event.accelerationIncludingGravity),
                    rotationRate: rates(event.rotationRate), interval: event.interval
                })));
            }
            return;
        }
        const state = sensorStates.get(subscription);
        if (!state || state.id !== id || state.phase === 'idle') return;
        if (event.kind === 'activated') {
            state.phase = 'activated';
            emit(subscription, new Event('activate'));
        } else if (event.kind === 'threeAxis') {
            if (state.phase !== 'activated' ||
                !['accelerometer', 'gyroscope', 'magnetometer'].includes(state.kind)) return;
            state.reading = { x: event.x, y: event.y, z: event.z };
            // WinRT device timestamps are only used by the broker to deduplicate samples.
            state.timestamp = performance.now();
            emit(subscription, new Event('reading'));
        } else if (event.kind === 'quaternion') {
            if (state.phase !== 'activated' ||
                !['absoluteOrientation', 'relativeOrientation'].includes(state.kind)) return;
            state.reading = { quaternion: Object.freeze([event.x, event.y, event.z, event.w]) };
            state.timestamp = performance.now();
            emit(subscription, new Event('reading'));
        } else if (event.kind === 'illuminance') {
            if (state.phase !== 'activated' || state.kind !== 'ambientLight') return;
            state.reading = { illuminance: event.illuminance };
            state.timestamp = performance.now();
            emit(subscription, new Event('reading'));
        } else if (event.kind === 'error') {
            subscriptions.delete(id);
            state.id = 0;
            state.phase = 'idle';
            state.reading = null;
            state.timestamp = null;
            emit(subscription, new SensorErrorEvent('error', {
                error: new DOMException('Unable to use the sensor', event.name)
            }));
        }
    };

    Object.assign(globalThis, { DeviceOrientationEvent, DeviceMotionEvent,
        DeviceMotionEventAcceleration, DeviceMotionEventRotationRate,
        Sensor, SensorErrorEvent, Accelerometer, Gyroscope, Magnetometer,
        AmbientLightSensor, OrientationSensor, AbsoluteOrientationSensor,
        RelativeOrientationSensor });
})();
