    // HTML StorageEvent and Web IDL dictionary conversion. Values are DOMStrings;
    // only url is a USVString, and it is not resolved against the document URL.
    // https://html.spec.whatwg.org/multipage/webstorage.html#the-storageevent-interface
    const storageEventSlots = new WeakMap();
    const storageEventState = receiver => {
        if (!storageEventSlots.has(receiver)) throw new TypeError('Incompatible StorageEvent receiver');
        return storageEventSlots.get(receiver);
    };
    const storageEventString = value => {
        if (typeof value === 'symbol') throw new TypeError('StorageEvent strings cannot be symbols');
        return String(value);
    };
    const storageEventNullableString = value => value == null ? null : storageEventString(value);
    const storageEventArea = value => {
        if (value == null) return null;
        if (!storageAreas.has(value)) throw new TypeError('storageArea must be a Storage object');
        return value;
    };
    const storageEventWellFormed = Function.call.bind(String.prototype.toWellFormed);
    const storageEventInitialize = Function.call.bind(Event.prototype.initEvent);
    class StorageEvent extends Event {
        constructor(type, init = {}) {
            if (!arguments.length) throw new TypeError('Missing StorageEvent type');
            type = storageEventString(type);
            if (init != null && typeof init !== 'object' && typeof init !== 'function') {
                throw new TypeError('StorageEventInit must be a dictionary');
            }
            init ??= {};
            // Web IDL reads inherited dictionaries first, and each dictionary's members
            // lexicographically. Read each member once, including accessor side effects.
            const bubbles = !!init.bubbles;
            const cancelable = !!init.cancelable;
            const composed = !!init.composed;
            const key = storageEventNullableString(init.key);
            const newValue = storageEventNullableString(init.newValue);
            const oldValue = storageEventNullableString(init.oldValue);
            const storageArea = storageEventArea(init.storageArea);
            const rawUrl = init.url;
            const url = storageEventWellFormed(rawUrl === undefined ? '' : storageEventString(rawUrl));
            super(type, { bubbles, cancelable, composed });
            storageEventSlots.set(this, { key, oldValue, newValue, url, storageArea });
        }
        get key() { return storageEventState(this).key; }
        get oldValue() { return storageEventState(this).oldValue; }
        get newValue() { return storageEventState(this).newValue; }
        get url() { return storageEventState(this).url; }
        get storageArea() { return storageEventState(this).storageArea; }
        initStorageEvent(type, bubbles = false, cancelable = false, key = null,
            oldValue = null, newValue = null, url = '', storageArea = null) {
            storageEventState(this);
            if (!arguments.length) throw new TypeError('Missing StorageEvent type');
            type = storageEventString(type);
            bubbles = !!bubbles;
            cancelable = !!cancelable;
            key = storageEventNullableString(key);
            oldValue = storageEventNullableString(oldValue);
            newValue = storageEventNullableString(newValue);
            url = storageEventWellFormed(storageEventString(url));
            storageArea = storageEventArea(storageArea);
            // Conversions still run while dispatching, but must not reinitialize the event.
            if (this.__dispatching) return;
            storageEventInitialize(this, type, bubbles, cancelable);
            trustedEvents.delete(this);
            storageEventSlots.set(this, { key, oldValue, newValue, url, storageArea });
        }
    }
    Object.defineProperty(StorageEvent.prototype, Symbol.toStringTag, {
        value: 'StorageEvent', configurable: true,
    });
    for (const name of ['key', 'oldValue', 'newValue', 'url', 'storageArea', 'initStorageEvent']) {
        Object.defineProperty(StorageEvent.prototype, name, { enumerable: true });
    }
    Object.defineProperty(windowObject, 'StorageEvent', {
        value: StorageEvent, writable: true, configurable: true,
    });
    defineEventHandler(windowObject, windowEvents, 'storage');
    const storageEventAreas = { local: windowObject.localStorage, session: windowObject.sessionStorage };
    const storageEventDispatch = Function.call.bind(windowObject.dispatchEvent, windowObject);
    // Captured by the embedder and removed from the global before page scripts run.
    windowObject.__dispatchStorageEvent = (area, key, oldValue, newValue, url) => {
        const event = new StorageEvent('storage');
        storageEventSlots.set(event, { key, oldValue, newValue, url, storageArea: storageEventAreas[area] });
        storageEventDispatch(markTrusted(event));
    };
