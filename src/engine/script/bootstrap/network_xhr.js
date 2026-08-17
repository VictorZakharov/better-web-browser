(() => {
    'use strict';
    const windowObject = globalThis;
    const markTrusted = globalThis.__markTrustedEvent;
    const forbiddenResponseHeaders = new Set(['set-cookie', 'set-cookie2']);
    const methodPattern = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const forbiddenMethods = new Set(['CONNECT', 'TRACE', 'TRACK']);
    const standardMethods = new Set(['DELETE', 'GET', 'HEAD', 'OPTIONS', 'POST', 'PUT']);
    const responseTypes = new Set(['', 'arraybuffer', 'blob', 'document', 'json', 'text']);
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
            this.__loaded = Math.max(0, Number(init.loaded) || 0);
            this.__total = Math.max(0, Number(init.total) || 0);
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
    const responseUrl = value => String(value).split('#', 1)[0];

    class XMLHttpRequestUpload extends EventTarget {}

    class XMLHttpRequest extends EventTarget {
        constructor() {
            super();
            this.__readyState = XMLHttpRequest.UNSENT;
            this.__response = null; this.__responseText = ''; this.__responseXML = null;
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
                ? this.__responseText : '';
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
            this.__responseURL = ''; this.__status = 0; this.__statusText = '';
            this.__responseHeaders = new Headers();
        }
        __cancelSilently() {
            if (this.__timeoutHandle) clearTimeout(this.__timeoutHandle);
            this.__timeoutHandle = 0;
            this.__send = false;
            this.__controller?.abort();
            this.__controller = null;
        }

        open(method, url, async = true, user = null, password = null) {
            method = normalizeMethod(method);
            const parsed = new URL(String(url));
            async = !!async;
            user = user === null ? null : String(user);
            password = password === null ? null : String(password);
            if (!async) throw new DOMException('Synchronous XMLHttpRequest is not supported', 'NotSupportedError');
            if (this.__send) this.__cancelSilently();
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
            this.__controller = new AbortController();
            this.__uploadTotal = request.__bodyBytes?.length || 0;
            this.__uploadComplete = body === null;
            this.dispatchEvent(progress('loadstart'));
            if (!this.__uploadComplete) this.__upload.dispatchEvent(progress('loadstart'));
            this.__armTimeout();
            fetch(request, { signal: this.__controller.signal }).then(
                response => this.__receive(response),
                error => {
                    if (!this.__send) return;
                    this.__requestError(error?.name === 'AbortError' ? 'abort' : 'error');
                }
            );
        }
        __armTimeout() {
            if (this.__timeoutHandle) clearTimeout(this.__timeoutHandle);
            this.__timeoutHandle = 0;
            if (!this.__send || this.__timeout === 0) return;
            this.__timeoutHandle = setTimeout(() => {
                if (!this.__send) return;
                this.__controller?.abort(new DOMException('The operation timed out', 'TimeoutError'));
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
        async __receive(response) {
            if (!this.__send) return;
            this.__finishUpload('load', this.__uploadTotal, this.__uploadTotal);
            this.__status = response.status; this.__statusText = response.statusText;
            this.__responseURL = responseUrl(response.url); this.__responseHeaders = response.headers;
            this.__changeState(XMLHttpRequest.HEADERS_RECEIVED);
            this.__changeState(XMLHttpRequest.LOADING);
            let bytes;
            try { bytes = await response.bytes(); }
            catch (_) { this.__requestError('error'); return; }
            if (!this.__send) return;
            const contentType = this.__mime || this.__responseHeaders.get('content-type') || '';
            const text = new TextDecoder().decode(bytes);
            this.__responseText = text;
            if (this.__responseType === 'arraybuffer')
                this.__response = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
            else if (this.__responseType === 'blob') this.__response = new Blob([bytes], { type: contentType });
            else if (this.__responseType === 'json') {
                try { this.__response = JSON.parse(text); } catch (_) { this.__response = null; }
            } else if (this.__responseType === 'document') {
                const parser = typeof DOMParser === 'function' ? new DOMParser() : null;
                try { this.__response = this.__responseXML = parser?.parseFromString(text, contentType) || null; }
                catch (_) { this.__response = this.__responseXML = null; }
            } else this.__response = text;
            const total = Number(this.__responseHeaders.get('content-length')) || 0;
            this.dispatchEvent(progress('progress', bytes.length, total));
            this.__finishSuccess(bytes.length, total);
        }
        abort() {
            const active = this.__send || this.__readyState === XMLHttpRequest.HEADERS_RECEIVED ||
                this.__readyState === XMLHttpRequest.LOADING;
            this.__controller?.abort();
            if (active) {
                this.__requestError('abort');
                this.__readyState = XMLHttpRequest.UNSENT;
            } else if (this.__readyState === XMLHttpRequest.DONE) {
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
            this.__clear(); this.__finishUpload(type);
            this.__resetResponse();
            this.__changeState(XMLHttpRequest.DONE);
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
