(() => {
    'use strict';
    const windowObject = globalThis;
    const markTrusted = globalThis.__markTrustedEvent;
    const receiveResponse = globalThis.__receiveXhrResponse;
    delete globalThis.__receiveXhrResponse;
    const forbiddenResponseHeaders = new Set(['set-cookie', 'set-cookie2']);
    const methodPattern = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const forbiddenMethods = new Set(['CONNECT', 'TRACE', 'TRACK']);
    const standardMethods = new Set(['DELETE', 'GET', 'HEAD', 'OPTIONS', 'POST', 'PUT']);
    const responseTypes = new Set(['', 'arraybuffer', 'blob', 'document', 'json', 'text']);
    const double = (value, fallback = 0) => {
        if (value === undefined) return fallback;
        const number = Number(value);
        if (!Number.isFinite(number)) throw new TypeError('ProgressEvent values must be finite');
        return number;
    };
    const byteString = (value, description) => {
        if (typeof value === 'symbol') throw new TypeError(description + ' is not a ByteString');
        const string = String(value);
        for (let index = 0; index < string.length; index++)
            if (string.charCodeAt(index) > 255) throw new TypeError(description + ' is not a ByteString');
        return string;
    };

    class ProgressEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.__lengthComputable = !!init.lengthComputable;
            this.__loaded = double(init.loaded);
            this.__total = double(init.total);
        }
        get lengthComputable() { return this.__lengthComputable; }
        get loaded() { return this.__loaded; }
        get total() { return this.__total; }
    }

    const eventHandler = (prototype, name) => Object.defineProperty(prototype, 'on' + name, {
        configurable: true, enumerable: true,
        get() { return this['__on' + name] || null; },
        set(value) {
            const key = '__listener' + name;
            if (this[key]) this.removeEventListener(name, this[key]);
            this['__on' + name] = typeof value === 'function' ? value : null;
            this[key] = this['__on' + name] ? event => this['__on' + name]?.call(this, event) : null;
            if (this[key]) this.addEventListener(name, this[key]);
        }
    });
    const progress = (type, loaded = 0, total = 0) => markTrusted(new ProgressEvent(type, {
        lengthComputable: total > 0, loaded, total
    }));
    const normalizeMethod = value => {
        const method = String(value);
        if (!methodPattern.test(method)) throw new DOMException('Invalid HTTP method', 'SyntaxError');
        const upper = method.toUpperCase();
        if (forbiddenMethods.has(upper)) throw new DOMException('Forbidden HTTP method', 'SecurityError');
        return standardMethods.has(upper) ? upper : method;
    };
    let failureDiagnostics = 0;

    class XMLHttpRequestUpload extends EventTarget {}

    class XMLHttpRequest extends EventTarget {
        constructor() {
            super();
            this.__readyState = XMLHttpRequest.UNSENT;
            this.__response = null; this.__responseText = ''; this.__responseXML = null;
            this.__responseTextChunks = []; this.__responseTextDirty = false;
            this.__responseType = ''; this.__responseURL = '';
            this.__status = 0; this.__statusText = '';
            this.__timeout = 0; this.__withCredentials = false;
            this.__headers = new Headers(); this.__responseHeaders = new Headers();
            this.__controller = null; this.__send = false; this.__timeoutHandle = 0;
            this.__uploadComplete = true; this.__uploadTotal = 0; this.__mime = null;
            this.__upload = new XMLHttpRequestUpload();
        }

        get readyState() { return this.__readyState; }
        get response() {
            if (this.__responseType === '' || this.__responseType === 'text') return this.responseText;
            return this.__readyState === XMLHttpRequest.DONE ? this.__response : null;
        }
        get responseText() {
            if (this.__responseType !== '' && this.__responseType !== 'text')
                throw new DOMException('responseText is unavailable for this responseType', 'InvalidStateError');
            return this.__readyState === XMLHttpRequest.LOADING || this.__readyState === XMLHttpRequest.DONE
                ? this.__materializeResponseText() : '';
        }
        get responseType() { return this.__responseType; }
        set responseType(value) {
            value = String(value);
            if (!responseTypes.has(value)) throw new TypeError('Invalid XMLHttpRequest responseType');
            if (this.__readyState === XMLHttpRequest.LOADING || this.__readyState === XMLHttpRequest.DONE)
                throw new DOMException('Response loading has already started', 'InvalidStateError');
            this.__responseType = value;
        }
        get responseURL() { return this.__responseURL; }
        get responseXML() {
            if (this.__responseType !== '' && this.__responseType !== 'document')
                throw new DOMException('responseXML is unavailable for this responseType', 'InvalidStateError');
            return this.__readyState === XMLHttpRequest.DONE ? this.__responseXML : null;
        }
        get status() { return this.__status; }
        get statusText() { return this.__statusText; }
        get timeout() { return this.__timeout; }
        set timeout(value) {
            value = Number(value);
            if (!Number.isFinite(value) || value < 0) throw new TypeError('timeout must be non-negative');
            this.__timeout = Math.min(0xffffffff, Math.trunc(value));
            if (this.__send) this.__armTimeout();
        }
        get withCredentials() { return this.__withCredentials; }
        set withCredentials(value) {
            if (this.__readyState !== XMLHttpRequest.UNSENT && this.__readyState !== XMLHttpRequest.OPENED || this.__send)
                throw new DOMException('withCredentials cannot be changed now', 'InvalidStateError');
            this.__withCredentials = !!value;
        }
        get upload() { return this.__upload; }

        __changeState(state) {
            this.__readyState = state;
            this.dispatchEvent(markTrusted(new Event('readystatechange')));
        }
        __resetResponse() {
            this.__response = null; this.__responseText = ''; this.__responseXML = null;
            this.__responseTextChunks = []; this.__responseTextDirty = false;
            this.__responseURL = ''; this.__status = 0; this.__statusText = '';
            this.__responseHeaders = new Headers();
        }
        __appendResponseText(text) {
            if (!text) return;
            this.__responseTextChunks.push(text);
            this.__responseTextDirty = true;
        }
        __materializeResponseText() {
            if (!this.__responseTextDirty) return this.__responseText;
            this.__responseText = this.__responseTextChunks.join('');
            // Collapse already-observed chunks so a later LOADING read only joins the newly
            // received suffix. Unobserved streaming responses retain linear append cost.
            this.__responseTextChunks = this.__responseText ? [this.__responseText] : [];
            this.__responseTextDirty = false;
            return this.__responseText;
        }
        __cancelSilently() {
            const controller = this.__controller;
            this.__clear();
            controller?.abort();
        }
        // open() terminates the previous fetch and its queued work. A reused object's
        // send flag is not ownership: old promise/reader callbacks can run after send().
        // https://xhr.spec.whatwg.org/#the-open()-method
        __isCurrent(controller) { return this.__send && this.__controller === controller; }

        open(method, url, async = true, user = null, password = null) {
            method = normalizeMethod(method);
            const parsed = new URL(String(url));
            async = !!async;
            user = user === null ? null : String(user);
            password = password === null ? null : String(password);
            if (!async) throw new DOMException('Synchronous XMLHttpRequest is not supported', 'NotSupportedError');
            this.__cancelSilently();
            const wasOpened = this.__readyState === XMLHttpRequest.OPENED;
            this.__method = method; this.__url = parsed.href;
            this.__user = user; this.__password = password;
            this.__headers = new Headers(); this.__mime = null;
            this.__uploadComplete = true; this.__uploadTotal = 0;
            this.__resetResponse();
            this.__readyState = XMLHttpRequest.OPENED;
            if (!wasOpened) this.dispatchEvent(markTrusted(new Event('readystatechange')));
        }
        setRequestHeader(name, value) {
            if (this.__readyState !== XMLHttpRequest.OPENED || this.__send)
                throw new DOMException('Request is not open for header changes', 'InvalidStateError');
            if (arguments.length < 2) throw new TypeError('setRequestHeader requires a value');
            name = byteString(name, 'Header name');
            value = byteString(value, 'Header value');
            try { this.__headers.append(name, value); }
            catch (error) {
                if (error instanceof TypeError)
                    throw new DOMException(error.message, 'SyntaxError');
                throw error;
            }
        }
        overrideMimeType(mime) {
            if (this.__readyState === XMLHttpRequest.LOADING || this.__readyState === XMLHttpRequest.DONE)
                throw new DOMException('Response loading has already started', 'InvalidStateError');
            mime = String(mime).trim();
            this.__mime = /^[^\s\/;]+\/[^\s\/;]+(?:\s*;.*)?$/.test(mime)
                ? mime : 'application/octet-stream';
        }
        getResponseHeader(name) {
            if (this.__readyState < XMLHttpRequest.HEADERS_RECEIVED) return null;
            name = String(name).toLowerCase();
            return forbiddenResponseHeaders.has(name) ? null : this.__responseHeaders.get(name);
        }
        getAllResponseHeaders() {
            if (this.__readyState < XMLHttpRequest.HEADERS_RECEIVED) return '';
            return [...this.__responseHeaders]
                .filter(([name]) => !forbiddenResponseHeaders.has(name))
                .map(([name, value]) => name + ': ' + value + '\r\n').join('');
        }

        send(body = null) {
            if (this.__readyState !== XMLHttpRequest.OPENED || this.__send)
                throw new DOMException('Request is not open', 'InvalidStateError');
            if (this.__method === 'GET' || this.__method === 'HEAD') body = null;
            const request = new Request(this.__url, {
                method: this.__method,
                headers: this.__headers,
                body,
                credentials: this.__withCredentials ? 'include' : 'same-origin'
            });
            this.__send = true;
            const controller = this.__controller = new AbortController();
            this.__uploadTotal = request.__bodyBytes?.length || 0;
            this.__uploadComplete = body === null;
            this.dispatchEvent(progress('loadstart'));
            if (!this.__isCurrent(controller)) return;
            if (!this.__uploadComplete) this.__upload.dispatchEvent(progress('loadstart'));
            if (!this.__isCurrent(controller)) return;
            this.__armTimeout();
            fetch(request, { signal: controller.signal }).then(
                response => receiveResponse(this, controller, response, progress, XMLHttpRequest),
                error => {
                    if (!this.__isCurrent(controller)) return;
                    this.__requestError(error?.name === 'AbortError' ? 'abort' : 'error');
                }
            );
        }
        __armTimeout() {
            if (this.__timeoutHandle) clearTimeout(this.__timeoutHandle);
            this.__timeoutHandle = 0;
            if (!this.__send || this.__timeout === 0) return;
            const controller = this.__controller;
            this.__timeoutHandle = setTimeout(() => {
                if (!this.__isCurrent(controller)) return;
                controller.abort(new DOMException('The operation timed out', 'TimeoutError'));
                this.__requestError('timeout');
            }, this.__timeout);
        }
        __finishUpload(type, loaded = 0, total = 0) {
            if (this.__uploadComplete) return;
            this.__uploadComplete = true;
            if (type === 'load') this.__upload.dispatchEvent(progress('progress', loaded, total));
            this.__upload.dispatchEvent(progress(type, loaded, total));
            this.__upload.dispatchEvent(progress('loadend', loaded, total));
        }
        abort() {
            const active = this.__send || this.__readyState === XMLHttpRequest.HEADERS_RECEIVED ||
                this.__readyState === XMLHttpRequest.LOADING;
            this.__controller?.abort();
            if (active) {
                this.__requestError('abort');
            }
            // An abort listener may already have opened/sent the next request.
            if (this.__readyState === XMLHttpRequest.DONE) {
                this.__resetResponse();
                this.__readyState = XMLHttpRequest.UNSENT;
            }
        }
        __clear() {
            if (this.__timeoutHandle) clearTimeout(this.__timeoutHandle);
            this.__timeoutHandle = 0; this.__send = false; this.__controller = null;
        }
        __finishSuccess(loaded, total) {
            this.__clear();
            this.__changeState(XMLHttpRequest.DONE);
            this.dispatchEvent(progress('load', loaded, total));
            this.dispatchEvent(progress('loadend', loaded, total));
        }
        __requestError(type) {
            if (!this.__send) return;
            if (type !== 'abort' && failureDiagnostics++ < 32)
                __hostCall('console', 'warn', 'XHR failed: ' + type + ' origin=' + new URL(this.__url).origin);
            const uploadIncomplete = !this.__uploadComplete;
            this.__uploadComplete = true;
            this.__clear();
            this.__resetResponse();
            this.__changeState(XMLHttpRequest.DONE);
            // Mutate all old-request state before exposing reentrant terminal events.
            if (uploadIncomplete) {
                this.__upload.dispatchEvent(progress(type));
                this.__upload.dispatchEvent(progress('loadend'));
            }
            this.dispatchEvent(progress(type));
            this.dispatchEvent(progress('loadend'));
        }
    }

    for (const [name, value] of Object.entries({
        UNSENT: 0, OPENED: 1, HEADERS_RECEIVED: 2, LOADING: 3, DONE: 4
    })) {
        Object.defineProperty(XMLHttpRequest, name, { enumerable: true, value });
        Object.defineProperty(XMLHttpRequest.prototype, name, { enumerable: true, value });
    }
    const progressEvents = ['loadstart', 'progress', 'abort', 'error', 'load', 'timeout', 'loadend'];
    eventHandler(XMLHttpRequest.prototype, 'readystatechange');
    for (const name of progressEvents) {
        eventHandler(XMLHttpRequest.prototype, name);
        eventHandler(XMLHttpRequestUpload.prototype, name);
    }
    Object.assign(windowObject, { ProgressEvent, XMLHttpRequest, XMLHttpRequestUpload });
})();
