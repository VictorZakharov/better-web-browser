(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const markTrusted = globalThis.__markTrustedEvent;
    const data = globalThis.__networkData;
    const body = globalThis.__networkBody;
    const { initializeBody, bodyUnusable, readBodyBytes, cloneBody, transferBody, installBody } = body;
    const standardMethods = new Set(['DELETE', 'GET', 'HEAD', 'OPTIONS', 'POST', 'PUT']);
    const forbiddenMethods = new Set(['CONNECT', 'TRACE', 'TRACK']);
    const methodPattern = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const modes = new Set(['same-origin', 'no-cors', 'cors']);
    const credentialsModes = new Set(['omit', 'same-origin', 'include']);
    const redirectModes = new Set(['follow', 'error', 'manual']);
    const cacheModes = new Set(['default', 'no-store', 'reload', 'no-cache', 'force-cache', 'only-if-cached']);
    const referrerPolicies = new Set([
        '', 'no-referrer', 'no-referrer-when-downgrade', 'same-origin', 'origin',
        'strict-origin', 'origin-when-cross-origin', 'strict-origin-when-cross-origin', 'unsafe-url'
    ]);
    const priorities = new Set(['high', 'low', 'auto']);
    const requestInitMembers = [
        'method', 'headers', 'body', 'referrer', 'referrerPolicy', 'mode', 'credentials',
        'cache', 'redirect', 'integrity', 'keepalive', 'signal', 'duplex', 'priority', 'window'
    ];
    const MAX_KEEPALIVE_BYTES = 64 * 1024;
    const MAX_REQUEST_BYTES = 16 * 1024 * 1024;
    const missingRequestInput = {};

    const abortSignalToken = {};
    const createAbortSignal = () => new AbortSignal(abortSignalToken);
    class AbortSignal extends EventTarget {
        constructor(token) {
            if (token !== abortSignalToken) throw new TypeError('Illegal constructor');
            super(); this.__aborted = false; this.__reason = undefined;
        }
        get aborted() { return this.__aborted; }
        get reason() { return this.__reason; }
        throwIfAborted() { if (this.__aborted) throw this.__reason; }
        __abort(reason = undefined) {
            if (this.__aborted) return;
            this.__aborted = true;
            this.__reason = reason === undefined
                ? new DOMException('The operation was aborted', 'AbortError') : reason;
            this.dispatchEvent(markTrusted(new Event('abort')));
        }
        static abort(reason = undefined) { const signal = createAbortSignal(); signal.__abort(reason); return signal; }
        static timeout(milliseconds) {
            milliseconds = Number(milliseconds);
            if (!Number.isFinite(milliseconds) || milliseconds < 0 || milliseconds > Number.MAX_SAFE_INTEGER)
                throw new RangeError('Timeout must be a finite non-negative integer');
            const signal = createAbortSignal();
            setTimeout(() => signal.__abort(new DOMException('The operation timed out', 'TimeoutError')),
                Math.trunc(milliseconds));
            return signal;
        }
        static any(signals) {
            const result = createAbortSignal();
            for (const signal of signals) {
                if (!(signal instanceof AbortSignal)) throw new TypeError('AbortSignal.any requires AbortSignal values');
                if (signal.aborted) { result.__abort(signal.reason); break; }
                signal.addEventListener('abort', () => result.__abort(signal.reason), { once: true });
            }
            return result;
        }
        static __follow(source) {
            if (!(source instanceof AbortSignal)) throw new TypeError('signal must be an AbortSignal');
            const follower = createAbortSignal();
            if (source.aborted) follower.__abort(source.reason);
            else source.addEventListener('abort', () => follower.__abort(source.reason), { once: true });
            return follower;
        }
    }
    Object.defineProperty(AbortSignal.prototype, 'onabort', {
        configurable: true, enumerable: true,
        get() { return this.__onabort || null; },
        set(value) {
            if (this.__onabortListener) this.removeEventListener('abort', this.__onabortListener);
            this.__onabort = typeof value === 'function' ? value : null;
            this.__onabortListener = this.__onabort ? event => this.__onabort?.call(this, event) : null;
            if (this.__onabortListener) this.addEventListener('abort', this.__onabortListener);
        }
    });
    class AbortController {
        constructor() { this.__signal = createAbortSignal(); }
        get signal() { return this.__signal; }
        abort(reason = undefined) { this.__signal.__abort(reason); }
    }

    const normalizedMethod = value => {
        const method = String(value);
        if (!methodPattern.test(method)) throw new TypeError('Invalid HTTP method');
        const upper = method.toUpperCase();
        if (forbiddenMethods.has(upper)) throw new TypeError('Forbidden HTTP method');
        return standardMethods.has(upper) ? upper : method;
    };
    const enumValue = (value, allowed, label) => {
        value = String(value);
        if (!allowed.has(value)) throw new TypeError('Invalid ' + label + ' value');
        return value;
    };
    const hasCredentials = url => /^[A-Za-z][A-Za-z0-9+.-]*:\/\/[^/?#]*@/.test(url);
    const optionValue = (init, name, fallback) => init[name] === undefined ? fallback : init[name];
    const hasRequestInit = init => requestInitMembers.some(name => init[name] !== undefined);

    class Request {
        constructor(input = missingRequestInput, init = {}) {
            if (input === missingRequestInput) throw new TypeError('Request requires an input');
            const source = input instanceof Request ? input : null;
            init = init == null ? {} : Object(init);
            if (init.window !== undefined && init.window !== null)
                throw new TypeError('RequestInit.window must be null');
            if (init.priority !== undefined) enumValue(init.priority, priorities, 'priority');
            const initNotEmpty = hasRequestInit(init);
            const replacementBody = Object.prototype.hasOwnProperty.call(init, 'body') && init.body != null;
            if (source && !replacementBody && bodyUnusable(source))
                throw new TypeError('Cannot construct from an unusable Request');
            const url = new URL(source ? source.url : String(input)).href;
            if (hasCredentials(url)) throw new TypeError('Request URL cannot include credentials');
            this.__url = url;
            this.__method = normalizedMethod(optionValue(init, 'method', source?.method ?? 'GET'));
            this.__mode = enumValue(optionValue(init, 'mode', source?.mode ?? 'cors'), modes, 'mode');
            if (this.__mode === 'no-cors' && !['GET', 'HEAD', 'POST'].includes(this.__method))
                throw new TypeError('no-cors requests require a CORS-safelisted method');
            this.__credentials = enumValue(optionValue(init, 'credentials', source?.credentials ?? 'same-origin'), credentialsModes, 'credentials');
            this.__redirect = enumValue(optionValue(init, 'redirect', source?.redirect ?? 'follow'), redirectModes, 'redirect');
            this.__cache = enumValue(optionValue(init, 'cache', source?.cache ?? 'default'), cacheModes, 'cache');
            if (this.__cache === 'only-if-cached' && this.__mode !== 'same-origin')
                throw new TypeError('only-if-cached requires same-origin mode');
            const fallbackPolicy = source && !initNotEmpty ? source.referrerPolicy : '';
            const fallbackReferrer = source && !initNotEmpty ? source.referrer : 'about:client';
            this.__referrerPolicy = enumValue(optionValue(init, 'referrerPolicy', fallbackPolicy), referrerPolicies, 'referrerPolicy');
            this.__referrer = String(optionValue(init, 'referrer', fallbackReferrer));
            if (this.__referrer !== '' && this.__referrer !== 'about:client') {
                this.__referrer = new URL(this.__referrer).href;
                if (new URL(this.__referrer).origin !== new URL(location.href).origin)
                    this.__referrer = 'about:client';
            }
            this.__integrity = String(optionValue(init, 'integrity', source?.integrity ?? ''));
            this.__keepalive = !!optionValue(init, 'keepalive', source?.keepalive ?? false);
            this.__signal = AbortSignal.__follow(optionValue(init, 'signal', source?.signal ?? createAbortSignal()) || createAbortSignal());
            this.__headers = new Headers();
            this.__headers.__setGuard(this.__mode === 'no-cors' ? 'request-no-cors' : 'request');
            this.__headers.__fill(optionValue(init, 'headers', source?.headers ?? {}));

            let extracted = replacementBody ? data.extractBody(init.body) : source
                ? source.__bodyBytes !== null
                    ? { bytes: new Uint8Array(source.__bodyBytes), stream: null, type: '' }
                    : { bytes: null, stream: source.__bodyStream, type: '' }
                : data.extractBody(null);
            if ((this.__method === 'GET' || this.__method === 'HEAD') && (extracted.bytes !== null || extracted.stream !== null))
                throw new TypeError(this.__method + ' requests cannot have a body');
            if (extracted.stream && replacementBody && init.duplex !== 'half')
                throw new TypeError("ReadableStream request bodies require duplex: 'half'");
            if (this.__keepalive && (extracted.stream || extracted.bytes?.length > MAX_KEEPALIVE_BYTES))
                throw new TypeError('keepalive request body is too large or streaming');
            if (extracted.type && !this.__headers.has('content-type')) this.__headers.set('content-type', extracted.type);
            if (source && source.__bodyStream !== null) {
                if (replacementBody) source.__bodyStream.__disturb();
                else extracted = transferBody(source);
            }
            initializeBody(this, extracted);
        }
        get url() { return this.__url; }
        get method() { return this.__method; }
        get headers() { return this.__headers; }
        get destination() { return ''; }
        get referrer() { return this.__referrer; }
        get referrerPolicy() { return this.__referrerPolicy; }
        get mode() { return this.__mode; }
        get credentials() { return this.__credentials; }
        get cache() { return this.__cache; }
        get redirect() { return this.__redirect; }
        get integrity() { return this.__integrity; }
        get keepalive() { return this.__keepalive; }
        get signal() { return this.__signal; }
        get duplex() { return 'half'; }
        get isReloadNavigation() { return false; }
        get isHistoryNavigation() { return false; }
        clone() {
            if (bodyUnusable(this)) throw new TypeError('Body has already been consumed');
            const clone = Object.create(Request.prototype);
            for (const name of ['url', 'method', 'mode', 'credentials', 'redirect', 'cache',
                'referrerPolicy', 'referrer', 'integrity', 'keepalive'])
                clone['__' + name] = this['__' + name];
            clone.__signal = AbortSignal.__follow(this.signal);
            clone.__headers = new Headers(this.headers)
                .__setGuard(this.__mode === 'no-cors' ? 'request-no-cors' : 'request');
            initializeBody(clone, cloneBody(this));
            return clone;
        }
        __serialize() {
            return readBodyBytes(this).then(bytes => {
                if (bytes.length > MAX_REQUEST_BYTES) throw new TypeError('Request body exceeds the browser limit');
                return {
                    url: this.url, method: this.method, headers: [...this.headers],
                    bodyBase64: this.body === null ? null : data.bytesToBase64(bytes),
                    mode: this.mode, credentials: this.credentials, redirect: this.redirect,
                    cache: this.cache, referrer: this.referrer, referrerPolicy: this.referrerPolicy,
                    integrity: this.integrity, keepalive: this.keepalive
                };
            });
        }
    }
    installBody(Request.prototype);

    class Response {
        constructor(body = null, init = {}) {
            init = init == null ? {} : Object(init);
            const status = init.status === undefined ? 200 : Number(init.status);
            if (!Number.isInteger(status) || status < 200 || status > 599)
                throw new RangeError('Response status must be between 200 and 599');
            const statusText = String(init.statusText || '');
            for (let index = 0; index < statusText.length; index++) {
                const code = statusText.charCodeAt(index);
                if (code > 255 || code !== 0x09 && (code < 0x20 || code === 0x7f))
                    throw new TypeError('Invalid response status text');
            }
            this.__status = status; this.__statusText = statusText;
            this.__type = 'default'; this.__url = ''; this.__redirected = false;
            this.__headers = new Headers(); this.__headers.__setGuard('response'); this.__headers.__fill(init.headers ?? {});
            const extracted = data.extractBody(body);
            if ([204, 205, 304].includes(status) && (extracted.bytes !== null || extracted.stream !== null))
                throw new TypeError('Null-body status cannot have a Response body');
            if (extracted.type && !this.__headers.has('content-type')) this.__headers.set('content-type', extracted.type);
            initializeBody(this, extracted);
        }
        get type() { return this.__type; }
        get url() { return this.__url; }
        get redirected() { return this.__redirected; }
        get status() { return this.__status; }
        get ok() { return this.__status >= 200 && this.__status <= 299; }
        get statusText() { return this.__statusText; }
        get headers() { return this.__headers; }
        clone() {
            if (bodyUnusable(this)) throw new TypeError('Body has already been consumed');
            const response = Object.create(Response.prototype);
            response.__status = this.status; response.__statusText = this.statusText;
            response.__type = this.type; response.__url = this.url; response.__redirected = this.redirected;
            response.__headers = new Headers(this.headers).__setGuard(this.headers.__guard);
            initializeBody(response, cloneBody(this));
            return response;
        }
        static error() { return Response.__fromNetwork({ status: 0, statusText: '', responseType: 'error', url: '', redirected: false, headers: [] }, null, true); }
        static redirect(url, status = 302) {
            status = Number(status);
            if (![301, 302, 303, 307, 308].includes(status)) throw new RangeError('Invalid redirect status');
            return new Response(null, { status, headers: { location: new URL(String(url)).href } });
        }
        static json(value, init = {}) {
            const body = JSON.stringify(value);
            if (body === undefined) throw new TypeError('Value is not JSON serializable');
            const headers = new Headers(init?.headers);
            if (!headers.has('content-type')) headers.set('content-type', 'application/json');
            return new Response(body, { ...init, headers });
        }
        static __fromNetwork(metadata, body, nullBody) {
            const response = Object.create(Response.prototype);
            response.__status = metadata.status; response.__statusText = metadata.statusText;
            response.__type = metadata.responseType; response.__url = metadata.url;
            response.__redirected = metadata.redirected;
            response.__headers = new Headers(metadata.headers
                .filter(([name]) => !['set-cookie', 'set-cookie2'].includes(String(name).toLowerCase())))
                .__setGuard('immutable');
            initializeBody(response, nullBody ? data.extractBody(null) : data.extractBody(body));
            return response;
        }
    }
    installBody(Response.prototype);

    Object.assign(globalThis, { Request, Response, AbortSignal, AbortController });
    delete globalThis.__networkBody;
    delete globalThis.__networkData;
})();
