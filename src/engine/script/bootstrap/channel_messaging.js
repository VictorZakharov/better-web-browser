    // In-realm channel messaging is task-queued on the document scheduler. Port endpoints are
    // separate from their JavaScript wrappers so transferring a port moves ownership without
    // disturbing its entanglement or queued messages.
    // https://html.spec.whatwg.org/multipage/web-messaging.html#channel-messaging
    const messagePortToken = {};
    const markPortEventTrusted = globalThis.__markTrustedEvent;
    const maxQueuedPortMessages = 256;
    const portEndpoint = owner => ({
        owner, entangled: null, queue: [], enabled: false, closed: false, scheduled: false
    });
    const portDataCloneError = () => {
        throw new DOMException('The value could not be cloned', 'DataCloneError');
    };
    const portTransfers = value => {
        if (value === undefined || value === null) return [];
        const source = Array.isArray(value) ? value : value.transfer;
        if (source === undefined || source === null) return [];
        if (typeof source[Symbol.iterator] !== 'function') throw new TypeError('transfer must be an iterable');
        const result = [...source], seen = new Set();
        for (const item of result) {
            if (seen.has(item)) portDataCloneError();
            seen.add(item);
            if (item instanceof MessagePort) {
                if (!item.__endpoint || item.__endpoint.closed) portDataCloneError();
            } else if (!(item instanceof ArrayBuffer) || item.detached) portDataCloneError();
        }
        return result;
    };
    const schedulePortDelivery = endpoint => {
        if (!endpoint.enabled || endpoint.closed || endpoint.scheduled || !endpoint.queue.length) return;
        endpoint.scheduled = true;
        setTimeout(() => {
            endpoint.scheduled = false;
            if (!endpoint.enabled || endpoint.closed || !endpoint.queue.length) return;
            const message = endpoint.queue.shift();
            endpoint.owner?.dispatchEvent(markPortEventTrusted(new MessageEvent('message', message)));
            schedulePortDelivery(endpoint);
        }, 0);
    };
    const transferMessagePort = port => {
        const endpoint = port.__endpoint;
        const transferred = new MessagePort(messagePortToken);
        port.__endpoint = null;
        transferred.__endpoint = endpoint;
        endpoint.owner = transferred;
        return transferred;
    };

    class MessagePort extends EventTarget {
        constructor(token) {
            if (token !== messagePortToken) throw new TypeError('Illegal constructor');
            super();
            this.__endpoint = portEndpoint(this);
        }
        postMessage(message, transferOrOptions = []) {
            const endpoint = this.__endpoint;
            if (!endpoint || endpoint.closed) return;
            const destination = endpoint.entangled;
            if (!destination || destination.closed) return;
            const transfers = portTransfers(transferOrOptions);
            if (transfers.includes(this)) portDataCloneError();
            if (destination.queue.length >= maxQueuedPortMessages)
                throw new DOMException('The MessagePort queue is full', 'QuotaExceededError');
            const buffers = transfers.filter(value => value instanceof ArrayBuffer);
            const cloned = structuredClone(message, { transfer: buffers });
            const ports = transfers.filter(value => value instanceof MessagePort)
                .map(transferMessagePort);
            destination.queue.push({ data: cloned, origin: '', source: null, ports });
            schedulePortDelivery(destination);
        }
        start() {
            const endpoint = this.__endpoint;
            if (!endpoint || endpoint.closed) return;
            endpoint.enabled = true;
            schedulePortDelivery(endpoint);
        }
        close() {
            const endpoint = this.__endpoint;
            if (!endpoint || endpoint.closed) return;
            endpoint.closed = true;
            endpoint.queue.length = 0;
            const other = endpoint.entangled;
            endpoint.entangled = null;
            if (other && other.entangled === endpoint) {
                other.entangled = null;
                setTimeout(() => other.owner?.dispatchEvent(markPortEventTrusted(new Event('close'))), 0);
            }
        }
        get onmessage() { return this.__onmessage || null; }
        set onmessage(value) { this.__setHandler('message', value); if (value != null) this.start(); }
        get onmessageerror() { return this.__onmessageerror || null; }
        set onmessageerror(value) { this.__setHandler('messageerror', value); if (value != null) this.start(); }
        get onclose() { return this.__onclose || null; }
        set onclose(value) { this.__setHandler('close', value); }
        __setHandler(type, value) {
            const property = '__on' + type;
            const previous = this[property];
            if (previous) this.removeEventListener(type, previous);
            this[property] = typeof value === 'function' ? value : null;
            if (this[property]) this.addEventListener(type, this[property]);
        }
    }
    Object.defineProperty(MessagePort.prototype, Symbol.toStringTag,
        { value: 'MessagePort', configurable: true });

    class MessageChannel {
        constructor() {
            const port1 = new MessagePort(messagePortToken);
            const port2 = new MessagePort(messagePortToken);
            port1.__endpoint.entangled = port2.__endpoint;
            port2.__endpoint.entangled = port1.__endpoint;
            Object.defineProperties(this, {
                port1: { enumerable: true, value: port1 },
                port2: { enumerable: true, value: port2 }
            });
        }
    }
    Object.defineProperty(MessageChannel.prototype, Symbol.toStringTag,
        { value: 'MessageChannel', configurable: true });
    Object.assign(globalThis, { MessageChannel, MessagePort });
