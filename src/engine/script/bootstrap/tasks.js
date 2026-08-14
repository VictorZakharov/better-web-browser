    const timers = new Map();
    const queueTimer = (callback, delay, repeat, args) => {
        const id = nextTimer++;
        delay = Math.max(0, Number(delay) || 0);
        timers.set(id, { callback, repeat, args });
        host('timerSchedule', id, delay, repeat);
        return id;
    };
    windowObject.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    windowObject.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    windowObject.clearTimeout = windowObject.clearInterval = id => {
        id = Number(id);
        timers.delete(id);
        host('timerCancel', id);
    };
    windowObject.requestAnimationFrame = callback => queueTimer(() => callback(performance.now()), 16, false, []);
    windowObject.cancelAnimationFrame = windowObject.clearTimeout;
    windowObject.queueMicrotask = callback => Promise.resolve().then(callback);
    windowObject.__runTimer = id => {
        const timer = timers.get(Number(id));
        if (!timer) return false;
        if (!timer.repeat) timers.delete(Number(id));
        if (typeof timer.callback === 'function') timer.callback(...timer.args);
        else (0, eval)(String(timer.callback));
        return true;
    };

    class URLSearchParams {
        constructor(init = '') {
            this.values = [];
            const source = String(init).replace(/^\?/, '');
            if (source) for (const part of source.split('&')) {
                const [key, value = ''] = part.split('=');
                this.values.push([decodeURIComponent(key.replace(/\+/g, ' ')), decodeURIComponent(value.replace(/\+/g, ' '))]);
            }
        }
        append(key, value) { this.values.push([String(key), String(value)]); }
        set(key, value) { this.delete(key); this.append(key, value); }
        get(key) { return this.values.find(entry => entry[0] === String(key))?.[1] ?? null; }
        getAll(key) { return this.values.filter(entry => entry[0] === String(key)).map(entry => entry[1]); }
        has(key) { return this.values.some(entry => entry[0] === String(key)); }
        delete(key) { key = String(key); this.values = this.values.filter(entry => entry[0] !== key); }
        toString() { return this.values.map(([key, value]) => encodeURIComponent(key) + '=' + encodeURIComponent(value)).join('&'); }
        entries() { return this.values[Symbol.iterator](); }
        keys() { return this.values.map(entry => entry[0])[Symbol.iterator](); }
        values() { return this.values.map(entry => entry[1])[Symbol.iterator](); }
        [Symbol.iterator]() { return this.entries(); }
    }
    windowObject.URLSearchParams = URLSearchParams;
    windowObject.URL = class URL {
        constructor(value, base = currentUrl) { this.href = host('resolveUrl', String(value || base)); }
        toString() { return this.href; }
        toJSON() { return this.href; }
        get protocol() { return parseUrl(this.href).protocol; }
        get host() { return parseUrl(this.href).host; }
        get hostname() { return parseUrl(this.href).hostname; }
        get pathname() { return parseUrl(this.href).pathname; }
        get search() { return parseUrl(this.href).search; }
        get hash() { return parseUrl(this.href).hash; }
        get origin() { const parsed = parseUrl(this.href); return parsed.protocol + '//' + parsed.host; }
        get searchParams() { return new URLSearchParams(this.search); }
    };

    const computedStyleProxy = element => new Proxy({
        getPropertyValue(name) {
            name = String(name).toLowerCase();
            const inline = element?.style?.getPropertyValue(name) || '';
            return inline || (element ? host('uaStyle', element.__id, name) : '');
        },
        get cssText() { return ''; }
    }, {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        }
    });
    windowObject.getComputedStyle = element => computedStyleProxy(element);
    windowObject.matchMedia = query => ({ media: String(query), matches: false, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; } });
    windowObject.CSS = { supports() { return false; }, escape(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, match => '\\' + match); } };
    windowObject.Image = class Image extends HTMLImageElement {
        constructor() {
            const element = document.createElement('img');
            Object.defineProperty(element, 'src', {
                configurable: true,
                get() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); },
                set(value) {
                    this.setAttribute('src', String(value));
                    setTimeout(() => this.dispatchEvent(new Event('error')), 0);
                }
            });
            return element;
        }
    };
    windowObject.MutationObserver = class { constructor(callback) { this.callback = callback; } observe() {} disconnect() {} takeRecords() { return []; } };
    windowObject.IntersectionObserver = class { constructor(callback) { this.callback = callback; } observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } };
    windowObject.ResizeObserver = class { constructor(callback) { this.callback = callback; } observe() {} unobserve() {} disconnect() {} };
    windowObject.fetch = () => Promise.reject(new TypeError('fetch is not implemented yet'));
    class XMLHttpRequest extends EventTarget {
        constructor() { super(); this.readyState = 0; this.status = 0; this.responseText = ''; }
        open(method, url) { this.method = method; this.url = host('resolveUrl', String(url)); this.readyState = 1; }
        setRequestHeader() {}
        send() { this.readyState = 4; this.dispatchEvent(new Event('error')); this.dispatchEvent(new Event('readystatechange')); }
        abort() {}
    }
    installEventHandlerAttributes(XMLHttpRequest.prototype);
    windowObject.XMLHttpRequest = XMLHttpRequest;
    windowObject.crypto = {
        getRandomValues(array) {
            for (let index = 0; index < array.length; index++) array[index] = Math.floor(Math.random() * 256);
            return array;
        },
        randomUUID() {
            return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, character => {
                const value = Math.floor(Math.random() * 16);
                return (character === 'x' ? value : (value & 3) | 8).toString(16);
            });
        }
    };

    windowObject.__wrap = wrap;
    windowObject.__finishDocument = () => {
        document.readyState = 'interactive';
        document.dispatchEvent(new Event('DOMContentLoaded'));
        document.readyState = 'complete';
        windowObject.dispatchEvent(new Event('load'));
    };
})();
