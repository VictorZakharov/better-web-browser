// HTML §9.5: browser-owned membership, sender exclusion, and structured clone.
// https://html.spec.whatwg.org/multipage/web-messaging.html#broadcasting-to-other-browsing-contexts
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('broadcastAvailable')) {
        delete globalThis.__installBroadcastListenerHook;
        return;
    }
    const serialize = globalThis.__serializeClone;
    const deserialize = globalThis.__deserializeClone;
    const trusted = globalThis.__markTrustedEvent;
    const Message = globalThis.MessageEvent;
    const dispatch = Function.call.bind(EventTarget.prototype.dispatchEvent);
    const states = new WeakMap(), weak = new Map(), roots = new Set();
    let nextId = 1;
    const collected = new FinalizationRegistry(id => {
        weak.delete(id);
        try { native('broadcastClose', id); } catch (_) { /* The document is already gone. */ }
    });
    const stateFor = channel => {
        const state = states.get(channel);
        if (!state) throw new TypeError('Invalid BroadcastChannel receiver');
        return state;
    };
    const encodedName = name => {
        if (name.length > 1024) throw new DOMException('BroadcastChannel name is too long', 'QuotaExceededError');
        let encoded = '';
        for (let index = 0; index < name.length; index++)
            encoded += name.charCodeAt(index).toString(16).padStart(4, '0');
        return encoded;
    };
    class BroadcastChannel extends EventTarget {
        constructor(name) {
            if (!arguments.length) throw new TypeError('BroadcastChannel requires a name');
            super();
            name = '' + name;
            if (nextId > 0xffffffff) throw new DOMException('BroadcastChannel identifier exhausted', 'QuotaExceededError');
            if (weak.size >= 128) throw new DOMException('BroadcastChannel limit reached', 'QuotaExceededError');
            const id = nextId++;
            native('broadcastOpen', id, encodedName(name));
            states.set(this, { id, name, closed: false, count: 0, message: null, messageerror: null });
            weak.set(id, new WeakRef(this));
            collected.register(this, id, this);
        }
        get name() { return stateFor(this).name; }
        postMessage(message) {
            if (!arguments.length) throw new TypeError('postMessage requires a message');
            const state = stateFor(this);
            if (state.closed) throw new DOMException('BroadcastChannel is closed', 'InvalidStateError');
            // BroadcastChannel has no transfer list. Existing clone support rejects ports and
            // other transfer-only objects and preserves cycles and built-in value types.
            const serialized = serialize(message);
            if (state.closed) throw new DOMException('BroadcastChannel is closed', 'InvalidStateError');
            if (serialized.length > 131072)
                throw new DOMException('BroadcastChannel message exceeds its limit', 'QuotaExceededError');
            native('broadcastPost', state.id, serialized);
        }
        close() {
            const state = stateFor(this);
            if (state.closed) return;
            native('broadcastClose', state.id);
            state.closed = true;
            roots.delete(this);
            weak.delete(state.id);
            collected.unregister(this);
        }
    }
    for (const type of ['message', 'messageerror']) {
        Object.defineProperty(BroadcastChannel.prototype, 'on' + type, {
            configurable: true, enumerable: true,
            get() { return stateFor(this)[type]; },
            set(value) {
                const state = stateFor(this);
                if (state[type]) this.removeEventListener(type, state[type]);
                state[type] = typeof value === 'function' ? value : null;
                if (state[type]) this.addEventListener(type, state[type]);
            }
        });
    }
    globalThis.__installBroadcastListenerHook((target, type, delta) => {
        if (type !== 'message' && type !== 'messageerror') return;
        const state = states.get(target);
        if (!state || state.closed) return;
        state.count += delta;
        if (state.count > 0) roots.add(target);
        else roots.delete(target);
    });
    delete globalThis.__installBroadcastListenerHook;
    globalThis.__dispatchBroadcastChannel = (id, serialized, origin) => {
        const target = weak.get(Number(id))?.deref();
        if (!target || stateFor(target).closed) return;
        try {
            dispatch(target, trusted(new Message('message', {
                data: deserialize(String(serialized)), origin: String(origin)
            })));
        } catch (_) {
            dispatch(target, trusted(new Message('messageerror', { origin: String(origin) })));
        }
    };
    Object.defineProperty(BroadcastChannel.prototype, Symbol.toStringTag,
        { value: 'BroadcastChannel', configurable: true });
    globalThis.BroadcastChannel = BroadcastChannel;
})();
