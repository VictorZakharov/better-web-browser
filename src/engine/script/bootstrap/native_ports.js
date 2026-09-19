// Private MessagePort facades. Native endpoints own entanglement and queued clones;
// author properties cannot forge an endpoint or expose another realm's callbacks.
(() => {
    const token = {}, handlers = new WeakMap();
    const trusted = globalThis.__markTrustedEvent;
    const EventConstructor = MessageEvent;
    let native;
    const transfers = options => {
        const source = Array.isArray(options) ? options : options?.transfer;
        if (source === undefined) return [];
        if (source === null || typeof source[Symbol.iterator] !== 'function')
            throw new TypeError('transfer must be iterable');
        return [...source];
    };
    class MessagePort extends EventTarget {
        constructor(key) {
            if (key !== token) throw new TypeError('Illegal constructor');
            super(); handlers.set(this, {});
        }
        postMessage(value, options) {
            if (!arguments.length) throw new TypeError('postMessage requires a message');
            native[1](this, value, transfers(options));
        }
        start() { native[2](this); }
        close() { native[3](this); }
    }
    for (const type of ['message', 'messageerror', 'close']) {
        Object.defineProperty(MessagePort.prototype, 'on' + type, {
            configurable: true, enumerable: true,
            get() { return handlers.get(this)?.[type] ?? null; },
            set(value) {
                const state = handlers.get(this);
                if (!state) throw new TypeError('Invalid MessagePort receiver');
                if (state[type]) this.removeEventListener(type, state[type]);
                state[type] = typeof value === 'function' ? value : null;
                if (state[type]) this.addEventListener(type, state[type]);
                if (type === 'message') this.start();
            }
        });
    }
    class MessageChannel {
        constructor() {
            const pair = native[0]();
            Object.defineProperties(this, {
                port1: {enumerable: true, value: pair[0]},
                port2: {enumerable: true, value: pair[1]}
            });
        }
    }
    Object.defineProperty(MessagePort.prototype, Symbol.toStringTag, {value:'MessagePort', configurable:true});
    Object.defineProperty(MessageChannel.prototype, Symbol.toStringTag, {value:'MessageChannel', configurable:true});
    globalThis.__nativePortBindings = [
        functions => { native = functions; },
        () => new MessagePort(token),
        (port, data, ports, closed) => port.dispatchEvent(trusted(closed
            ? new Event('close') : new EventConstructor('message', {data, ports, origin:'', source:null})))
    ];
    Object.assign(globalThis, {MessagePort, MessageChannel});
})();
