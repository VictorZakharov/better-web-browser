// Private MessagePort facades. Native endpoints own entanglement and queued clones;
// author properties cannot forge an endpoint or expose another realm's callbacks.
(() => {
    const token = {}, handlers = new WeakMap(), remote = new WeakMap(), received = new Map();
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
            const endpoint = remote.get(this);
            if (endpoint) {
                if (!endpoint.detached && !endpoint.closed) {
                    const list = transfers(options);
                    if (list.includes(this))
                        throw new DOMException('A port cannot transfer itself', 'DataCloneError');
                    for (const port of list) {
                        if (remote.has(port) && remote.get(port).worker !== endpoint.worker)
                            throw new DOMException('Ports belong to different Workers', 'DataCloneError');
                    }
                    __hostCall('workerPortPostMessage', endpoint.worker, endpoint.id,
                        __serializeClone(value, list));
                }
                return;
            }
            native[1](this, value, transfers(options));
        }
        start() {
            const endpoint = remote.get(this);
            if (!endpoint) return native[2](this);
            if (endpoint.detached || endpoint.closed) return;
            endpoint.enabled = true;
            while (endpoint.queue.length) dispatchRemote(this, endpoint, endpoint.queue.shift());
        }
        close() {
            const endpoint = remote.get(this);
            if (!endpoint) return native[3](this);
            if (endpoint.detached || endpoint.closed) return;
            endpoint.closed = true;
            endpoint.queue.length = 0;
            received.delete(endpoint.worker + ':' + endpoint.id);
            __hostCall('workerPortClose', endpoint.worker, endpoint.id);
        }
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
    const dispatchRemote = (port, endpoint, item) => {
        if (item.closed) {
            endpoint.closed = true;
            port.dispatchEvent(trusted(new Event('close')));
            return;
        }
        try {
            const {data, ports} = __deserializeCloneWithPorts(item.payload, endpoint.worker);
            port.dispatchEvent(trusted(new EventConstructor('message', {data, ports, origin:'', source:null})));
        } catch (_) { port.dispatchEvent(trusted(new Event('messageerror'))); }
    };
    const receive = (descriptor, worker) => {
        worker ??= descriptor.worker;
        if (!Number.isSafeInteger(worker) || !Number.isSafeInteger(descriptor.id) ||
            (descriptor.worker !== undefined && descriptor.worker !== worker))
            throw new DOMException('Invalid port', 'DataCloneError');
        const key = worker + ':' + descriptor.id;
        if (received.has(key)) throw new DOMException('Port is already owned', 'DataCloneError');
        const port = new MessagePort(token);
        const endpoint = {worker, id:descriptor.id, enabled:false, closed:false, detached:false, queue:[]};
        remote.set(port, endpoint);
        received.set(key, port);
        return port;
    };
    globalThis.__clonePortBindings = {
        isPort: value => value instanceof MessagePort,
        describe: value => {
            const endpoint = remote.get(value);
            if (!endpoint || endpoint.detached || endpoint.closed)
                throw new DOMException('The MessagePort cannot be transferred to this Worker', 'DataCloneError');
            return {id:endpoint.id, worker:endpoint.worker};
        },
        detach: value => {
            const endpoint = remote.get(value);
            if (!endpoint || endpoint.detached) throw new DOMException('Detached MessagePort', 'DataCloneError');
            endpoint.detached = true;
            received.delete(endpoint.worker + ':' + endpoint.id);
        },
        receive
    };
    globalThis.__workerPortBridge = {
        deliver(worker, id, payload, closed) {
            const port = received.get(worker + ':' + id);
            if (!port) return;
            const endpoint = remote.get(port), item = {payload, closed};
            if (!endpoint || endpoint.detached || endpoint.closed) return;
            if (!endpoint.enabled && !closed) {
                if (endpoint.queue.length < 256) endpoint.queue.push(item);
            } else dispatchRemote(port, endpoint, item);
        },
        terminate(worker) {
            for (const [key, port] of received) {
                const endpoint = remote.get(port);
                if (endpoint?.worker !== worker) continue;
                endpoint.closed = true;
                endpoint.queue.length = 0;
                received.delete(key);
                port.dispatchEvent(trusted(new Event('close')));
            }
        }
    };
    globalThis.__nativePortBindings = [
        functions => { native = functions; },
        () => new MessagePort(token),
        (port, data, ports, closed) => port.dispatchEvent(trusted(closed
            ? new Event('close') : new EventConstructor('message', {data, ports, origin:'', source:null})))
    ];
    Object.assign(globalThis, {MessagePort, MessageChannel});
})();
