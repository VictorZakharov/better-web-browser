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
        requestMediaKeySystemAccess() {
            return Promise.reject(new DOMException(
                'Encrypted media playback is not supported',
                'NotSupportedError'
            ));
        },
        sendBeacon(url) { host('console', 'beacon', String(url)); return false; },
        javaEnabled() { return false; }
    };
    const validPositiveMediaNumber = value => Number.isFinite(Number(value)) && Number(value) > 0;
    const mediaConfigurationSnapshot = configuration => {
        if (configuration == null || typeof configuration !== 'object' ||
            !['file', 'media-source', 'webrtc'].includes(configuration.type) ||
            (!configuration.audio && !configuration.video)) {
            throw new TypeError('Invalid media decoding configuration');
        }
        const snapshot = { type: configuration.type };
        if (configuration.video) {
            const video = configuration.video;
            if (typeof video.contentType !== 'string' ||
                !validPositiveMediaNumber(video.width) ||
                !validPositiveMediaNumber(video.height) ||
                !validPositiveMediaNumber(video.bitrate) ||
                !validPositiveMediaNumber(video.framerate)) {
                throw new TypeError('Invalid video decoding configuration');
            }
            snapshot.video = { ...video };
        }
        if (configuration.audio) {
            const audio = configuration.audio;
            if (typeof audio.contentType !== 'string' || typeof audio.channels !== 'string' ||
                !audio.channels || !validPositiveMediaNumber(audio.bitrate) ||
                !validPositiveMediaNumber(audio.samplerate)) {
                throw new TypeError('Invalid audio decoding configuration');
            }
            snapshot.audio = { ...audio };
        }
        if (configuration.keySystemConfiguration)
            snapshot.keySystemConfiguration = { ...configuration.keySystemConfiguration };
        return snapshot;
    };
    windowObject.navigator.mediaCapabilities = {
        decodingInfo(configuration) {
            let snapshot;
            try { snapshot = mediaConfigurationSnapshot(configuration); }
            catch (error) { return Promise.reject(error); }
            const contentSupported = source => !source || supportedMediaType(source.contentType) !== '';
            const supported = snapshot.type !== 'webrtc' &&
                !snapshot.keySystemConfiguration &&
                contentSupported(snapshot.video) && contentSupported(snapshot.audio);
            const smooth = supported && (!snapshot.video || (
                Number(snapshot.video.width) <= 1920 && Number(snapshot.video.height) <= 1080 &&
                Number(snapshot.video.framerate) <= 60
            ));
            return Promise.resolve({
                supported,
                smooth,
                // The current backend does not prove a hardware or otherwise power-optimal path.
                powerEfficient: false,
                keySystemAccess: null,
                configuration: snapshot
            });
        }
    };
    iframeWindow.navigator = windowObject.navigator;
    windowObject.screen = { width: 1280, height: 720, availWidth: 1280, availHeight: 680, colorDepth: 24, pixelDepth: 24 };
    windowObject.innerWidth = 1280;
    windowObject.innerHeight = 720;
    windowObject.devicePixelRatio = 1;
    windowObject.scrollX = windowObject.pageXOffset = 0;
    windowObject.scrollY = windowObject.pageYOffset = 0;
    windowObject.scrollTo = windowObject.scrollBy = () => {};

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
    const decoderInputView = input => {
        if (input === undefined) return new Uint8Array();
        if (input instanceof ArrayBuffer) return new Uint8Array(input);
        if (ArrayBuffer.isView?.(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
        throw new TypeError('input must be an ArrayBuffer or an ArrayBuffer view');
    };
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            label = String(label).trim().toLowerCase();
            if (!['utf-8', 'utf8', 'unicode-1-1-utf-8'].includes(label)) {
                throw new RangeError('Only UTF-8 decoding is implemented');
            }
            this.__fatal = !!options.fatal;
            this.__ignoreBOM = !!options.ignoreBOM;
            this.__pending = new Uint8Array();
            this.__streaming = false;
            this.__bomSeen = false;
        }
        get encoding() { return 'utf-8'; }
        get fatal() { return this.__fatal; }
        get ignoreBOM() { return this.__ignoreBOM; }
        decode(input, options = {}) {
            const stream = !!options.stream;
            const result = host(
                'utf8Decode',
                decoderInputView(input),
                this.__streaming ? this.__pending : new Uint8Array(),
                stream,
                this.__fatal,
                this.__ignoreBOM,
                this.__bomSeen
            );
            this.__pending = result[1];
            this.__streaming = stream;
            this.__bomSeen = result[2];
            if (!stream) {
                this.__pending = new Uint8Array();
                this.__bomSeen = false;
            }
            return result[0];
        }
    }
    windowObject.TextEncoder = TextEncoder;
    windowObject.TextDecoder = TextDecoder;
    const makeConsole = level => (...args) => host('console', level, args.map(value => {
        try {
            if (typeof value === 'string') return value;
            if (value instanceof Error) return value.stack || value.message || String(value);
            return JSON.stringify(value);
        }
        catch (_) { return String(value); }
    }).join(' '));
    windowObject.console = {
        log: makeConsole('log'), info: makeConsole('info'), warn: makeConsole('warn'),
        error: makeConsole('error'), debug: makeConsole('debug'), trace: makeConsole('trace'),
        assert(condition, ...args) { if (!condition) makeConsole('assert')(...args); },
        time() {}, timeEnd() {}, group() {}, groupEnd() {}
    };

    let nextTimer = 1;
