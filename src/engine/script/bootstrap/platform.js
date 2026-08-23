    windowObject.history = {
        length: 1,
        state: null,
        pushState(state, _title, url) { this.state = state; if (url != null) currentUrl = host('resolveUrl', String(url)); },
        replaceState(state, _title, url) { this.state = state; if (url != null) currentUrl = host('resolveUrl', String(url)); },
        back() {}, forward() {}, go() {}
    };

    const storage = area => {
        return {
            get length() { return host('storageLength', area); },
            key(index) { return host('storageKey', area, Number(index) >>> 0); },
            getItem(key) { return host('storageGet', area, String(key)); },
            setItem(key, value) { host('storageSet', area, String(key), String(value)); },
            removeItem(key) { host('storageRemove', area, String(key)); },
            clear() { host('storageClear', area); }
        };
    };
    windowObject.localStorage = storage('local');
    windowObject.sessionStorage = storage('session');
    windowObject.navigator = {
        userAgent: host('userAgent'),
        appName: 'Netscape',
        appVersion: '5.0',
        platform: 'Win32',
        language: 'en-CA',
        languages: ['en-CA', 'en'],
        onLine: true,
        cookieEnabled: true,
        hardwareConcurrency: 1,
        maxTouchPoints: 0,
        sendBeacon(url) { host('console', 'beacon', String(url)); return false; },
        javaEnabled() { return false; }
    };
    iframeWindow.navigator = windowObject.navigator;
    windowObject.screen = { width: 1280, height: 720, availWidth: 1280, availHeight: 680, colorDepth: 24, pixelDepth: 24 };
    windowObject.innerWidth = 1280;
    windowObject.innerHeight = 720;
    windowObject.devicePixelRatio = 1;
    windowObject.scrollX = windowObject.pageXOffset = 0;
    windowObject.scrollY = windowObject.pageYOffset = 0;
    windowObject.scrollTo = windowObject.scrollBy = () => {};

    const started = Date.now();
    windowObject.performance = {
        timeOrigin: started,
        now() { return Date.now() - started; },
        mark() {}, measure() {}, getEntriesByType() { return []; },
        timing: { navigationStart: started }
    };
    const base64Alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    windowObject.atob = value => {
        const input = String(value).replace(/[\t\n\f\r ]/g, '').replace(/=+$/, '');
        if (input.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(input)) throw new Error('InvalidCharacterError');
        let bits = 0, bitCount = 0, output = '';
        for (const character of input) {
            bits = (bits << 6) | base64Alphabet.indexOf(character);
            bitCount += 6;
            if (bitCount >= 8) {
                bitCount -= 8;
                output += String.fromCharCode((bits >> bitCount) & 255);
            }
        }
        return output;
    };
    windowObject.btoa = value => {
        const input = String(value);
        let output = '', buffer = 0, bitCount = 0;
        for (let index = 0; index < input.length; index++) {
            const code = input.charCodeAt(index);
            if (code > 255) throw new Error('InvalidCharacterError');
            buffer = (buffer << 8) | code;
            bitCount += 8;
            while (bitCount >= 6) {
                bitCount -= 6;
                output += base64Alphabet[(buffer >> bitCount) & 63];
            }
        }
        if (bitCount > 0) output += base64Alphabet[(buffer << (6 - bitCount)) & 63];
        while (output.length % 4) output += '=';
        return output;
    };
    const unicodeScalarAt = (input, index) => {
        const first = input.charCodeAt(index);
        if (first >= 0xD800 && first <= 0xDBFF && index + 1 < input.length) {
            const second = input.charCodeAt(index + 1);
            if (second >= 0xDC00 && second <= 0xDFFF) {
                return [0x10000 + ((first - 0xD800) << 10) + second - 0xDC00, 2];
            }
        }
        return [first >= 0xD800 && first <= 0xDFFF ? 0xFFFD : first, 1];
    };
    const utf8Bytes = scalar => {
        if (scalar <= 0x7F) return [scalar];
        if (scalar <= 0x7FF) return [0xC0 | (scalar >> 6), 0x80 | (scalar & 0x3F)];
        if (scalar <= 0xFFFF) {
            return [0xE0 | (scalar >> 12), 0x80 | ((scalar >> 6) & 0x3F), 0x80 | (scalar & 0x3F)];
        }
        return [
            0xF0 | (scalar >> 18),
            0x80 | ((scalar >> 12) & 0x3F),
            0x80 | ((scalar >> 6) & 0x3F),
            0x80 | (scalar & 0x3F)
        ];
    };
    class TextEncoder {
        get encoding() { return 'utf-8'; }
        encode(input = '') {
            input = String(input);
            const output = [];
            for (let index = 0; index < input.length;) {
                const [scalar, units] = unicodeScalarAt(input, index);
                output.push(...utf8Bytes(scalar));
                index += units;
            }
            return new Uint8Array(output);
        }
        encodeInto(source, destination) {
            source = String(source);
            if (!(destination instanceof Uint8Array)) throw new TypeError('destination must be a Uint8Array');
            let read = 0;
            let written = 0;
            while (read < source.length) {
                const [scalar, units] = unicodeScalarAt(source, read);
                const bytes = utf8Bytes(scalar);
                if (written + bytes.length > destination.length) break;
                destination.set(bytes, written);
                written += bytes.length;
                read += units;
            }
            return { read, written };
        }
    }
    const decoderInputBytes = input => {
        if (input === undefined) return [];
        if (input instanceof ArrayBuffer) return [...new Uint8Array(input)];
        if (ArrayBuffer.isView?.(input)) return [...new Uint8Array(input.buffer, input.byteOffset, input.byteLength)];
        throw new TypeError('input must be an ArrayBuffer or an ArrayBuffer view');
    };
    const scalarString = scalar => scalar <= 0xFFFF
        ? String.fromCharCode(scalar)
        : String.fromCharCode(0xD800 + ((scalar - 0x10000) >> 10), 0xDC00 + ((scalar - 0x10000) & 0x3FF));
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            label = String(label).trim().toLowerCase();
            if (!['utf-8', 'utf8', 'unicode-1-1-utf-8'].includes(label)) {
                throw new RangeError('Only UTF-8 decoding is implemented');
            }
            this.__fatal = !!options.fatal;
            this.__ignoreBOM = !!options.ignoreBOM;
            this.__pending = [];
            this.__streaming = false;
            this.__bomSeen = false;
        }
        get encoding() { return 'utf-8'; }
        get fatal() { return this.__fatal; }
        get ignoreBOM() { return this.__ignoreBOM; }
        decode(input, options = {}) {
            const stream = !!options.stream;
            const bytes = (this.__streaming ? this.__pending : []).concat(decoderInputBytes(input));
            this.__pending = [];
            let output = '';
            let index = 0;
            const emit = scalar => {
                if (!this.__bomSeen) {
                    this.__bomSeen = true;
                    if (!this.__ignoreBOM && scalar === 0xFEFF) return;
                }
                output += scalarString(scalar);
            };
            const fail = () => {
                if (this.__fatal) throw new TypeError('The encoded data was not valid UTF-8');
                emit(0xFFFD);
            };
            while (index < bytes.length) {
                const first = bytes[index];
                if (first <= 0x7F) {
                    emit(first);
                    index++;
                    continue;
                }
                let needed = 0;
                let scalar = 0;
                let minimum = 0;
                if (first >= 0xC2 && first <= 0xDF) {
                    needed = 1; scalar = first & 0x1F; minimum = 0x80;
                } else if (first >= 0xE0 && first <= 0xEF) {
                    needed = 2; scalar = first & 0x0F; minimum = 0x800;
                } else if (first >= 0xF0 && first <= 0xF4) {
                    needed = 3; scalar = first & 0x07; minimum = 0x10000;
                } else {
                    fail();
                    index++;
                    continue;
                }
                if (index + needed >= bytes.length) {
                    if (stream) this.__pending = bytes.slice(index);
                    else fail();
                    index = bytes.length;
                    break;
                }
                let valid = true;
                for (let offset = 1; offset <= needed; offset++) {
                    const continuation = bytes[index + offset];
                    if ((continuation & 0xC0) !== 0x80) { valid = false; break; }
                    scalar = (scalar << 6) | (continuation & 0x3F);
                }
                if (!valid || scalar < minimum || scalar > 0x10FFFF || (scalar >= 0xD800 && scalar <= 0xDFFF)) {
                    fail();
                    index++;
                    continue;
                }
                emit(scalar);
                index += needed + 1;
            }
            this.__streaming = stream;
            if (!stream) {
                this.__pending = [];
                this.__bomSeen = false;
            }
            return output;
        }
    }
    windowObject.TextEncoder = TextEncoder;
    windowObject.TextDecoder = TextDecoder;
    const makeConsole = level => (...args) => host('console', level, args.map(value => {
        try { return typeof value === 'string' ? value : JSON.stringify(value); }
        catch (_) { return String(value); }
    }).join(' '));
    windowObject.console = {
        log: makeConsole('log'), info: makeConsole('info'), warn: makeConsole('warn'),
        error: makeConsole('error'), debug: makeConsole('debug'), trace: makeConsole('trace'),
        assert(condition, ...args) { if (!condition) makeConsole('assert')(...args); },
        time() {}, timeEnd() {}, group() {}, groupEnd() {}
    };

    let nextTimer = 1;
