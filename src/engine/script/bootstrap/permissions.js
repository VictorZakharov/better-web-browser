// Permissions API queries browser-owned grants; querying never prompts.
// https://w3c.github.io/permissions/#permissions-api
(() => {
    'use strict';
    const host = globalThis.__hostCall;
    const trustedEvent = globalThis.__markTrustedEvent;
    const token = {};
    const pending = new Map();
    const statuses = new Map();
    const slots = new WeakMap();
    const names = new Set([
        'notifications', 'geolocation', 'accelerometer',
        'gyroscope', 'magnetometer', 'ambient-light-sensor'
    ]);
    const keyFor = name => name;
    const descriptorName = value => {
        if (value === undefined || typeof value === 'symbol')
            throw new TypeError('PermissionDescriptor.name is required');
        return String(value);
    };

    class PermissionStatus extends EventTarget {
        constructor(key, name, state) {
            if (key !== token) throw new TypeError('Illegal constructor');
            super();
            slots.set(this, {name, state});
            this.onchange = null;
        }
        get name() {
            const slot = slots.get(this);
            if (!slot) throw new TypeError('Illegal PermissionStatus invocation');
            return slot.name;
        }
        get state() {
            const slot = slots.get(this);
            if (!slot) throw new TypeError('Illegal PermissionStatus invocation');
            return slot.state;
        }
    }
    class Permissions {
        constructor(key) {
            if (key !== token) throw new TypeError('Illegal constructor');
        }
        query(descriptor) {
            if (this !== permissions) return Promise.reject(new TypeError('Illegal Permissions invocation'));
            try {
                if (!host('permissionIsActive'))
                    throw new DOMException('Document is not fully active', 'InvalidStateError');
                if (arguments.length === 0 || descriptor === null || typeof descriptor !== 'object')
                    throw new TypeError('PermissionDescriptor must be an object');
                const rootName = descriptorName(descriptor.name);
                if (!names.has(rootName)) throw new TypeError('Unsupported permission name');
                // The generic descriptor and the selected feature's typed descriptor
                // each perform their own WebIDL dictionary conversion.
                const name = descriptorName(descriptor.name);
                if (!names.has(name)) throw new TypeError('Unsupported permission name');
                return new Promise((resolve, reject) => {
                    try {
                        const id = Number(host('permissionQuery', name));
                        pending.set(id, {resolve, reject, name});
                    } catch (error) { reject(error); }
                });
            } catch (error) { return Promise.reject(error); }
        }
    }
    const permissions = new Permissions(token);
    Object.defineProperty(navigator, 'permissions', {
        configurable: true, enumerable: true, get: () => permissions
    });
    globalThis.Permissions = Permissions;
    globalThis.PermissionStatus = PermissionStatus;
    globalThis.__receivePermissionUpdate = payload => {
        const update = JSON.parse(String(payload));
        const name = String(update.name);
        const state = String(update.state);
        if (!names.has(name) || !['granted', 'denied', 'prompt'].includes(state)) return;
        if (update.rejected) {
            const item = pending.get(Number(update.id));
            if (item) {
                pending.delete(Number(update.id));
                item.reject(new DOMException('Permission query limit exceeded', 'QuotaExceededError'));
            }
            return;
        }
        const key = keyFor(name);
        if (pending.has(Number(update.id))) {
            const item = pending.get(Number(update.id));
            pending.delete(Number(update.id));
            if (item.name !== name) {
                item.reject(new DOMException('Permission response mismatch', 'UnknownError'));
                return;
            }
            const status = new PermissionStatus(token, name, state);
            let list = statuses.get(key);
            if (!list) statuses.set(key, list = []);
            // A PermissionStatus with a change listener must remain alive; queries
            // are bounded and the realm drops these references on document retirement.
            list.push(status);
            item.resolve(status);
            return;
        }
        const list = statuses.get(key);
        if (!list) return;
        for (const status of list) {
            const slot = slots.get(status);
            if (slot.state === state) continue;
            slot.state = state;
            const event = trustedEvent(new Event('change'));
            status.dispatchEvent(event);
            if (typeof status.onchange === 'function') status.onchange.call(status, event);
        }
    };
})();
