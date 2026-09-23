(() => {
    'use strict';
    // HTML §9.2: https://html.spec.whatwg.org/multipage/server-sent-events.html
    // An author replacing window.fetch must not replace EventSource's transport.
    const fetchNetwork = globalThis.fetch;
    const URLConstructor = globalThis.URL;
    const TextDecoderConstructor = globalThis.TextDecoder;
    const EventConstructor = globalThis.Event;
    const MessageEventConstructor = globalThis.MessageEvent;
    const AbortControllerConstructor = globalThis.AbortController;
    const schedule = globalThis.setTimeout;
    const unschedule = globalThis.clearTimeout;
    const dispatch = EventTarget.prototype.dispatchEvent;
    const states = new WeakMap();
    const MAX_EVENT_CHARS = 1024 * 1024;
    const MAX_QUEUED_EVENTS = 4096;
    const DEFAULT_RETRY_MS = 3000;
    const MAX_RETRY_MS = 2147483647;

    const stateFor = receiver => {
        const state = states.get(receiver);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const trusted = globalThis.__markTrustedEvent;
    const queueEvent = (source, state, event) => {
        if (++state.queuedEvents > MAX_QUEUED_EVENTS) {
            state.queuedEvents--;
            throw new RangeError('EventSource event queue exceeds the browser limit');
        }
        schedule(() => {
            state.queuedEvents--;
            if (state.readyState !== EventSource.CLOSED) dispatch.call(source, trusted(event));
        }, 0);
    };
    const terminate = (source, state) => {
        if (state.readyState === EventSource.CLOSED) return;
        schedule(() => {
            if (state.readyState === EventSource.CLOSED) return;
            state.readyState = EventSource.CLOSED;
            state.generation++;
            state.abort?.abort();
            if (state.retryTimer !== null) unschedule(state.retryTimer);
            dispatch.call(source, trusted(new EventConstructor('error')));
        }, 0);
    };
    const reconnect = (source, state, generation) => {
        if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
        schedule(() => {
            if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
            state.readyState = EventSource.CONNECTING;
            dispatch.call(source, trusted(new EventConstructor('error')));
            if (state.readyState !== EventSource.CONNECTING) return;
            state.retryTimer = schedule(() => {
                state.retryTimer = null;
                if (state.readyState === EventSource.CONNECTING && state.generation === generation)
                    connect(source, state);
            }, state.retry);
        }, 0);
    };

    const createParser = (source, state, response, generation) => {
        let line = '', data = '', type = '', eventId = state.lastEventId;
        let afterCR = false;
        const origin = new URLConstructor(response.url || state.url).origin;
        const dispatch = () => {
            state.lastEventId = eventId;
            if (data !== '') {
                const event = new MessageEventConstructor(type || 'message', {
                    data: data.slice(0, -1), origin, lastEventId: state.lastEventId
                });
                queueEvent(source, state, event);
            }
            data = '';
            type = '';
        };
        const processLine = () => {
            const current = line;
            line = '';
            if (current === '') { dispatch(); return; }
            if (current[0] === ':') return;
            const colon = current.indexOf(':');
            const field = colon === -1 ? current : current.slice(0, colon);
            let value = colon === -1 ? '' : current.slice(colon + 1);
            if (value[0] === ' ') value = value.slice(1);
            switch (field) {
                case 'data': data += value + '\n'; break;
                case 'event': type = value; break;
                case 'id': if (!value.includes('\0')) eventId = value; break;
                case 'retry':
                    if (/^[0-9]+$/.test(value)) state.retry = Math.min(Number(value), MAX_RETRY_MS);
                    break;
            }
            if (data.length > MAX_EVENT_CHARS) throw new RangeError('EventSource event exceeds the browser limit');
        };
        return text => {
            for (let index = 0; index < text.length; index++) {
                if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
                const character = text[index];
                if (afterCR) {
                    afterCR = false;
                    if (character === '\n') continue;
                }
                if (character === '\r' || character === '\n') {
                    processLine();
                    afterCR = character === '\r';
                } else {
                    line += character;
                    if (line.length > MAX_EVENT_CHARS)
                        throw new RangeError('EventSource line exceeds the browser limit');
                }
            }
        };
    };

    const readStream = async (source, state, response, generation) => {
        const decoder = new TextDecoderConstructor('utf-8');
        const parse = createParser(source, state, response, generation);
        const reader = response.body?.getReader();
        if (!reader) return;
        try {
            while (state.readyState !== EventSource.CLOSED && state.generation === generation) {
                const { value, done } = await reader.read();
                if (done) break;
                parse(decoder.decode(value, { stream: true }));
            }
            // EOF discards an unterminated event, but flushes decoder state.
            if (state.readyState !== EventSource.CLOSED && state.generation === generation)
                parse(decoder.decode());
        } finally {
            reader.releaseLock();
        }
    };

    const connect = (source, state) => {
        const generation = ++state.generation;
        const abort = new AbortControllerConstructor();
        state.abort = abort;
        const headers = { Accept: 'text/event-stream' };
        if (state.lastEventId !== '') headers['Last-Event-ID'] = state.lastEventId;
        fetchNetwork(state.url, {
            headers, signal: abort.signal, cache: 'no-store', mode: 'cors',
            credentials: state.withCredentials ? 'include' : 'same-origin'
        }).then(async response => {
            if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
            const mime = response.headers.get('content-type')?.split(';', 1)[0].trim().toLowerCase();
            if (response.status !== 200 || mime !== 'text/event-stream') {
                // HTTP 204 and all non-event-stream responses are terminal.
                terminate(source, state);
                return;
            }
            schedule(() => {
                if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
                state.readyState = EventSource.OPEN;
                dispatch.call(source, trusted(new EventConstructor('open')));
            }, 0);
            try {
                await readStream(source, state, response, generation);
                reconnect(source, state, generation);
            } catch (error) {
                if (state.readyState === EventSource.CLOSED || state.generation !== generation) return;
                // A local resource limit is terminal. Network stream errors may recover.
                if (error instanceof RangeError) terminate(source, state);
                else reconnect(source, state, generation);
            }
        }, () => reconnect(source, state, generation));
    };

    class EventSource extends EventTarget {
        constructor(url, init = {}) {
            super();
            if (arguments.length === 0) throw new TypeError('EventSource requires a URL');
            let absolute;
            try { absolute = new URLConstructor(String(url), location.href).href; }
            catch (_) { throw new DOMException('Invalid EventSource URL', 'SyntaxError'); }
            init = init == null ? {} : Object(init);
            const state = {
                url: absolute, withCredentials: !!init.withCredentials,
                readyState: EventSource.CONNECTING, retry: DEFAULT_RETRY_MS,
                retryTimer: null, abort: null, generation: 0, lastEventId: '',
                handlers: {}, queuedEvents: 0
            };
            states.set(this, state);
            connect(this, state);
        }
        get url() { return stateFor(this).url; }
        get withCredentials() { return stateFor(this).withCredentials; }
        get readyState() { return stateFor(this).readyState; }
        close() {
            const state = stateFor(this);
            if (state.readyState === EventSource.CLOSED) return;
            state.readyState = EventSource.CLOSED;
            state.generation++;
            state.abort?.abort();
            if (state.retryTimer !== null) unschedule(state.retryTimer);
        }
    }
    for (const [name, value] of Object.entries({ CONNECTING: 0, OPEN: 1, CLOSED: 2 })) {
        Object.defineProperty(EventSource, name, { value, enumerable: true });
        Object.defineProperty(EventSource.prototype, name, { value, enumerable: true });
    }
    for (const type of ['open', 'message', 'error']) {
        Object.defineProperty(EventSource.prototype, 'on' + type, {
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
    Object.defineProperty(EventSource.prototype, Symbol.toStringTag, { value: 'EventSource' });
    globalThis.EventSource = EventSource;
})();
