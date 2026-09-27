// Window Notifications are brokered by the browser, not by renderer script.
// https://notifications.spec.whatwg.org/#api
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    const trusted = globalThis.__markTrustedEvent || (event => event);
    const pendingPermissions = new Map();
    const notifications = new Map();
    let permission = String(native('notificationPermission'));

    function emit(target, type) {
        const event = trusted(new Event(type));
        target.dispatchEvent(event);
        const handler = target['on' + type];
        if (typeof handler === 'function') handler.call(target, event);
    }
    function request(command) {
        return Number(native('notificationRequest', JSON.stringify(command)));
    }
    class Notification extends EventTarget {
        static get permission() { return permission; }
        static get maxActions() { return 0; }
        static requestPermission(callback) {
            if (callback !== undefined && typeof callback !== 'function')
                throw new TypeError('Notification permission callback must be a function');
            return new Promise((resolve, reject) => {
                try {
                    const id = request({ kind: 'requestPermission' });
                    pendingPermissions.set(id, { resolve, callback });
                } catch (error) { reject(error); }
            });
        }
        constructor(title, options = {}) {
            super();
            if (arguments.length === 0) throw new TypeError('Notification requires a title');
            if (options == null || typeof options !== 'object')
                throw new TypeError('Notification options must be an object');
            // Actions, images, sound, and persistence are not represented by the Win32
            // balloon backend. Reject rather than silently claiming those capabilities.
            if (options.actions?.length || options.image || options.icon || options.badge ||
                options.vibrate !== undefined || options.silent || options.requireInteraction ||
                'data' in options || options.renotify || options.timestamp !== undefined ||
                (options.dir && options.dir !== 'auto') || options.lang)
                throw new DOMException('Notification option is not supported', 'NotSupportedError');
            this.title = String(title);
            this.body = String(options.body ?? '');
            this.tag = String(options.tag ?? '');
            this.dir = 'auto';
            this.lang = '';
            this.onclick = null;
            this.onshow = null;
            this.onerror = null;
            this.onclose = null;
            this._id = request({ kind: 'show', title: this.title, body: this.body, tag: this.tag });
            notifications.set(this._id, this);
        }
        close() {
            if (this._id) request({ kind: 'close', id: this._id });
        }
    }
    globalThis.__receiveNotificationUpdate = payload => {
        const update = JSON.parse(String(payload));
        const id = Number(update.id);
        if (update.kind === 'permission') {
            permission = update.permission;
            const pending = pendingPermissions.get(id);
            pendingPermissions.delete(id);
            if (pending) {
                pending.resolve(permission);
                if (pending.callback) pending.callback(permission);
            }
            return;
        }
        const notification = notifications.get(id);
        if (!notification) return;
        if (update.kind === 'closed' || update.kind === 'error') {
            notifications.delete(id);
            notification._id = 0;
        }
        const types = { shown: 'show', clicked: 'click', closed: 'close', error: 'error' };
        if (types[update.kind]) emit(notification, types[update.kind]);
    };
    globalThis.Notification = Notification;
})();
