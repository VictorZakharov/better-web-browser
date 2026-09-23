(() => {
    'use strict';
    // WHATWG WebSockets: URL/subprotocol validation precedes any broker work.
    // Only the browser process owns WinHTTP handles and network credentials.
    // https://websockets.spec.whatwg.org/#the-websocket-interface
    const host = (...args) => __hostCall(...args);
    const sockets = new Map();
    const states = new WeakMap();
    const dispatch = EventTarget.prototype.dispatchEvent;
    const trusted = globalThis.__markTrustedEvent;
    const encoder = new TextEncoder();
    const decoder = new TextDecoder('utf-8', { fatal: true });
    const protocolToken = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const stateFor = socket => {
        const state = states.get(socket);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const toBase64 = bytes => {
        let binary = '';
        for (let offset = 0; offset < bytes.length; offset += 8192)
            binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
        return btoa(binary);
    };
    class CloseEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.wasClean = !!init.wasClean;
            this.code = Number(init.code) || 0;
            this.reason = String(init.reason ?? '');
        }
    }
    Object.defineProperty(CloseEvent.prototype, Symbol.toStringTag, { value: 'CloseEvent' });
    globalThis.CloseEvent = CloseEvent;

    class WebSocket extends EventTarget {
        constructor(url, protocols = []) {
            super();
            if (arguments.length === 0) throw new TypeError('WebSocket requires a URL');
            const rawUrl = String(url);
            let parsed;
            try { parsed = new URL(rawUrl, location.href); }
            catch (_) { throw new DOMException('Invalid WebSocket URL', 'SyntaxError'); }
            if (rawUrl.includes('#') || parsed.hash || parsed.username || parsed.password)
                throw new DOMException('Invalid WebSocket URL', 'SyntaxError');
            if (parsed.protocol === 'http:') parsed.protocol = 'ws:';
            else if (parsed.protocol === 'https:') parsed.protocol = 'wss:';
            else if (parsed.protocol !== 'ws:' && parsed.protocol !== 'wss:')
                throw new DOMException('Invalid WebSocket scheme', 'SyntaxError');
            const offered = typeof protocols === 'string' ? [protocols] : [...protocols];
            const seen = new Set();
            for (let index = 0; index < offered.length; index++) {
                offered[index] = String(offered[index]);
                if (!protocolToken.test(offered[index]) || seen.has(offered[index]))
                    throw new DOMException('Invalid WebSocket subprotocol', 'SyntaxError');
                seen.add(offered[index]);
            }
            const state = {
                id: Number(host('webSocketOpen', parsed.href, JSON.stringify(offered))),
                url: parsed.href, readyState: WebSocket.CONNECTING, protocol: '',
                extensions: '', binaryType: 'blob', bufferedAmount: 0, handlers: {}
            };
            states.set(this, state);
            sockets.set(state.id, this);
        }
        get url() { return stateFor(this).url; }
        get readyState() { return stateFor(this).readyState; }
        get protocol() { return stateFor(this).protocol; }
        get extensions() { return stateFor(this).extensions; }
        get bufferedAmount() { return stateFor(this).bufferedAmount; }
        get binaryType() { return stateFor(this).binaryType; }
        set binaryType(value) {
            value = String(value);
            if (value === 'blob' || value === 'arraybuffer') stateFor(this).binaryType = value;
        }
        send(data) {
            const state = stateFor(this);
            if (arguments.length === 0) throw new TypeError('WebSocket.send requires data');
            if (state.readyState === WebSocket.CONNECTING)
                throw new DOMException('WebSocket is still connecting', 'InvalidStateError');
            if (state.readyState !== WebSocket.OPEN) return;
            let bytes, binary;
            if (data instanceof ArrayBuffer) {
                bytes = new Uint8Array(data); binary = true;
            } else if (ArrayBuffer.isView(data)) {
                bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength); binary = true;
            } else if (data instanceof Blob) {
                // Blob serialization is async, so enqueue after its immutable byte
                // snapshot while retaining the order of successive sends.
                const size = data.size;
                state.bufferedAmount += size;
                state.blobChain = (state.blobChain || Promise.resolve())
                    .then(() => data.arrayBuffer()).then(buffer => {
                    host('webSocketSend', state.id, true, toBase64(new Uint8Array(buffer)));
                });
                return;
            } else {
                data = String(data);
                bytes = encoder.encode(data); binary = false;
            }
            state.bufferedAmount += bytes.length;
            const payload = binary ? toBase64(bytes) : data;
            if (state.blobChain)
                state.blobChain = state.blobChain.then(() =>
                    host('webSocketSend', state.id, binary, payload));
            else
                host('webSocketSend', state.id, binary, payload);
        }
        close(code = 1000, reason = '') {
            const state = stateFor(this);
            if (arguments.length && !(code === 1000 || Number.isInteger(code) && code >= 3000 && code <= 4999))
                throw new DOMException('Invalid WebSocket close code', 'InvalidAccessError');
            reason = String(reason);
            if (encoder.encode(reason).length > 123)
                throw new DOMException('WebSocket close reason is too long', 'SyntaxError');
            if (state.readyState === WebSocket.CLOSING || state.readyState === WebSocket.CLOSED) return;
            state.readyState = WebSocket.CLOSING;
            if (state.blobChain)
                state.blobChain = state.blobChain.then(() =>
                    host('webSocketClose', state.id, code, reason));
            else
                host('webSocketClose', state.id, code, reason);
        }
    }
    for (const [name, value] of Object.entries({ CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3 })) {
        Object.defineProperty(WebSocket, name, { value, enumerable: true });
        Object.defineProperty(WebSocket.prototype, name, { value, enumerable: true });
    }
    for (const type of ['open', 'message', 'error', 'close']) {
        Object.defineProperty(WebSocket.prototype, 'on' + type, {
            enumerable: true, configurable: true,
            get() { return stateFor(this).handlers[type]?.value || null; },
            set(value) {
                const state = stateFor(this);
                let handler = state.handlers[type];
                if (!handler) {
                    handler = { value: null };
                    state.handlers[type] = handler;
                    this.addEventListener(type, event => {
                        if (typeof handler.value === 'function') handler.value.call(this, event);
                    });
                }
                handler.value = typeof value === 'function' ? value : null;
            }
        });
    }
    Object.defineProperty(WebSocket.prototype, Symbol.toStringTag, { value: 'WebSocket' });
    globalThis.WebSocket = WebSocket;

    globalThis.__receiveWebSocketEvent = (id, kind, payload, metadata) => {
        const socket = sockets.get(Number(id));
        if (!socket) return;
        const state = states.get(socket);
        if (kind === 'open') {
            if (state.readyState !== WebSocket.CONNECTING) return;
            state.readyState = WebSocket.OPEN;
            state.protocol = String(payload);
            dispatch.call(socket, trusted(new Event('open')));
        } else if (kind === 'message') {
            if (state.readyState !== WebSocket.OPEN) return;
            const binary = !!metadata;
            const data = binary
                ? state.binaryType === 'arraybuffer'
                    ? Uint8Array.from(payload).buffer
                    : new Blob([Uint8Array.from(payload)])
                : decoder.decode(Uint8Array.from(payload));
            dispatch.call(socket, trusted(new MessageEvent('message', {
                data, origin: new URL(state.url).origin
            })));
        } else if (kind === 'sent') {
            state.bufferedAmount = Math.max(0, state.bufferedAmount - Number(payload));
        } else if (kind === 'error') {
            if (state.readyState === WebSocket.CLOSED) return;
            dispatch.call(socket, trusted(new Event('error')));
        } else if (kind === 'close') {
            if (state.readyState === WebSocket.CLOSED) return;
            state.readyState = WebSocket.CLOSED;
            state.bufferedAmount = 0;
            sockets.delete(state.id);
            dispatch.call(socket, trusted(new CloseEvent('close', JSON.parse(String(payload)))));
        }
    };
})();
