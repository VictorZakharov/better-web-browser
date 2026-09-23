// A dedicated worker owns its channel endpoints. Transfer moves the wrapper to
// the document agent; only endpoint identifiers and cloned data cross threads.
// HTML § 9.4: https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports
(() => {
    'use strict';
    const token = {}, endpoints = new Map(), wrappers = new WeakMap();
    const trusted = globalThis.__markTrustedEvent;
    let nextId = 1;
    const fail = () => { throw new DOMException('The MessagePort could not be transferred', 'DataCloneError'); };
    const transfers = options => {
        const source = Array.isArray(options) ? options : options?.transfer;
        if (source === undefined) return [];
        if (source === null || typeof source[Symbol.iterator] !== 'function')
            throw new TypeError('transfer must be iterable');
        return [...source];
    };
    const active = port => {
        const binding = wrappers.get(port);
        if (!binding) throw new TypeError('Invalid MessagePort receiver');
        return binding;
    };
    const deliver = (endpoint, serialized) => {
        if (endpoint.closed) return;
        if (!endpoint.enabled || endpoint.owner !== 'worker') {
            if (endpoint.queue.length < 256) endpoint.queue.push(serialized);
            return;
        }
        setTimeout(() => {
            if (endpoint.closed || endpoint.owner !== 'worker') return;
            try {
                const {data, ports} = __deserializeCloneWithPorts(serialized);
                endpoint.port.dispatchEvent(trusted(new MessageEvent('message', {data, ports})));
            } catch (_) {
                endpoint.port.dispatchEvent(trusted(new MessageEvent('messageerror')));
            }
        }, 0);
    };
    const flush = endpoint => {
        while (endpoint.enabled && endpoint.owner === 'worker' && endpoint.queue.length)
            deliver(endpoint, endpoint.queue.shift());
    };
    const wrap = endpoint => {
        const port = new MessagePort(token);
        wrappers.set(port, {endpoint, detached: false});
        endpoint.port = port;
        endpoint.owner = 'worker';
        flush(endpoint);
        return port;
    };
    class MessagePort extends EventTarget {
        constructor(key) {
            if (key !== token) throw new TypeError('Illegal constructor');
            super();
        }
        postMessage(message, options) {
            if (!arguments.length) throw new TypeError('postMessage requires a message');
            const {endpoint, detached} = active(this);
            if (detached || endpoint.closed) return;
            const list = transfers(options);
            if (list.includes(this)) fail();
            const peer = endpoints.get(endpoint.peer);
            const doomed = peer?.port && list.includes(peer.port);
            const serialized = __serializeClone(message, list);
            if (!peer || peer.closed || doomed) return;
            if (peer.owner === 'worker') deliver(peer, serialized);
            else __hostCall('workerPortPost', peer.id, serialized);
        }
        start() {
            const {endpoint, detached} = active(this);
            if (!detached && !endpoint.closed) { endpoint.enabled = true; flush(endpoint); }
        }
        close() {
            const {endpoint, detached} = active(this);
            if (detached || endpoint.closed) return;
            endpoint.closed = true;
            endpoint.queue.length = 0;
            const peer = endpoints.get(endpoint.peer);
            if (peer && !peer.closed) {
                peer.closed = true;
                if (peer.owner === 'worker')
                    peer.port.dispatchEvent(trusted(new Event('close')));
                else __hostCall('workerPortClosed', peer.id);
            }
        }
    }
    for (const type of ['message', 'messageerror', 'close']) {
        Object.defineProperty(MessagePort.prototype, 'on' + type, {
            configurable: true, enumerable: true,
            get() { return active(this)[type] ?? null; },
            set(value) {
                const binding = active(this);
                if (binding[type]) this.removeEventListener(type, binding[type]);
                binding[type] = typeof value === 'function' ? value : null;
                if (binding[type]) this.addEventListener(type, binding[type]);
                if (type === 'message') this.start();
            }
        });
    }
    class MessageChannel {
        constructor() {
            if (endpoints.size >= 4094) throw new DOMException('MessagePort limit reached', 'QuotaExceededError');
            const first = nextId++, second = nextId++;
            const a = {id:first, peer:second, owner:'worker', enabled:false, closed:false, queue:[]};
            const b = {id:second, peer:first, owner:'worker', enabled:false, closed:false, queue:[]};
            endpoints.set(first, a); endpoints.set(second, b);
            Object.defineProperties(this, {
                port1: {enumerable:true, value:wrap(a)},
                port2: {enumerable:true, value:wrap(b)}
            });
        }
    }
    Object.defineProperty(MessagePort.prototype, Symbol.toStringTag, {value:'MessagePort', configurable:true});
    Object.defineProperty(MessageChannel.prototype, Symbol.toStringTag, {value:'MessageChannel', configurable:true});
    globalThis.__clonePortBindings = {
        isPort: value => wrappers.has(value),
        describe: value => {
            const binding = wrappers.get(value);
            if (!binding || binding.detached || binding.endpoint.closed ||
                binding.endpoint.owner !== 'worker') return fail();
            return {id:binding.endpoint.id};
        },
        detach: value => {
            const binding = wrappers.get(value);
            if (!binding || binding.detached) return fail();
            binding.detached = true;
            binding.endpoint.owner = 'window';
            binding.endpoint.port = null;
        },
        receive: descriptor => {
            const endpoint = endpoints.get(Number(descriptor.id));
            if (!endpoint || endpoint.closed || endpoint.owner !== 'window') return fail();
            return wrap(endpoint);
        }
    };
    globalThis.__dispatchWorkerPortMessage = (sourceId, serialized) => {
        const source = endpoints.get(Number(sourceId));
        const peer = source && endpoints.get(source.peer);
        if (source?.owner === 'window' && peer?.owner === 'worker')
            deliver(peer, String(serialized));
    };
    globalThis.__dispatchWorkerPortClose = sourceId => {
        const source = endpoints.get(Number(sourceId));
        const peer = source && endpoints.get(source.peer);
        if (source?.owner !== 'window' || !peer || peer.closed) return;
        source.closed = peer.closed = true;
        peer.queue.length = 0;
        peer.port.dispatchEvent(trusted(new Event('close')));
    };
    Object.assign(globalThis, {MessagePort, MessageChannel});
})();
