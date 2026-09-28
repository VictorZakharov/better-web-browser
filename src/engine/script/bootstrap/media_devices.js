// Partial MediaDevices surface: pre-capture enumeration only. No getUserMedia
// or devicechange claim until capture and hotplug observation are implemented.
// https://www.w3.org/TR/mediacapture-streams/#enumerating-devices
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('mediaDevicesAvailable')) return;
    const pending = new Map();
    const internal = {};

    class MediaDeviceInfo {
        constructor(key, kind) {
            if (key !== internal) throw new TypeError('Illegal constructor');
            Object.defineProperties(this, {
                deviceId: { value: '', enumerable: true },
                kind: { value: kind, enumerable: true },
                label: { value: '', enumerable: true },
                groupId: { value: '', enumerable: true }
            });
        }
        toJSON() {
            return { deviceId: this.deviceId, kind: this.kind,
                label: this.label, groupId: this.groupId };
        }
    }
    class InputDeviceInfo extends MediaDeviceInfo {
        constructor(key, kind) { super(key, kind); }
        // No capture grant has occurred, so detailed capabilities are unavailable.
        getCapabilities() { return {}; }
    }
    class MediaDevices extends EventTarget {
        constructor(key) {
            super();
            if (key !== internal) throw new TypeError('Illegal constructor');
        }
        enumerateDevices() {
            return new Promise((resolve, reject) => {
                try {
                    const id = Number(native('mediaDeviceRequest'));
                    pending.set(id, { resolve, reject });
                } catch (error) { reject(error); }
            });
        }
    }
    const mediaDevices = new MediaDevices(internal);
    Object.defineProperty(navigator, 'mediaDevices', {
        configurable: true, enumerable: true, get: () => mediaDevices
    });
    globalThis.MediaDevices = MediaDevices;
    globalThis.MediaDeviceInfo = MediaDeviceInfo;
    globalThis.InputDeviceInfo = InputDeviceInfo;
    globalThis.__receiveMediaDeviceUpdate = payload => {
        const update = JSON.parse(String(payload));
        const item = pending.get(Number(update.id));
        if (!item) return;
        pending.delete(Number(update.id));
        if (update.error) {
            item.reject(new DOMException(update.message, update.error));
            return;
        }
        const devices = [];
        if (update.microphone) devices.push(new InputDeviceInfo(internal, 'audioinput'));
        if (update.camera) devices.push(new InputDeviceInfo(internal, 'videoinput'));
        item.resolve(devices);
    };
})();
