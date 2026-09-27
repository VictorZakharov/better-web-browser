    let historyLength = 1;
    let historyIndex = 0;
    let historyState = null;
    const historyCapacity = host('historyCapacity');
    const advanceHistoryIndex = () => {
        historyIndex = Math.min(historyIndex + 1, historyCapacity - 1);
        historyLength = historyIndex + 1;
    };
    let serializeHistoryState;
    let deserializeHistoryState;
    Object.defineProperty(windowObject, '__setHistoryStateCloneBindings', {
        configurable: true,
        value(serialize, deserialize) {
            serializeHistoryState = serialize;
            deserializeHistoryState = deserialize;
        }
    });
    class PopStateEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            Object.defineProperties(this, {
                state: { enumerable: true, value: init.state === undefined ? null : init.state },
                hasUAVisualTransition: { enumerable: true, value: !!init.hasUAVisualTransition }
            });
        }
    }
    windowObject.PopStateEvent = PopStateEvent;
    const firePopState = () => windowObject.dispatchEvent(markTrusted(
        new PopStateEvent('popstate', { state: historyState })
    ));
    const historyString = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to a string');
        return String(value);
    };
    const updateHistoryState = (state, url, replace) => {
        // HTML serializes before checking the new URL, and stores a snapshot rather than
        // retaining the caller's object. Keep that serialization in the native entry.
        // https://html.spec.whatwg.org/multipage/nav-history-apis.html#shared-history-push/replace-state-steps
        const serialized = serializeHistoryState(state);
        const snapshot = deserializeHistoryState(serialized);
        const target = url == null || url === '' ? currentUrl : url;
        const resolved = host('historyUpdate', target, replace, serialized);
        currentUrl = resolved;
        historyState = snapshot;
        if (!replace) advanceHistoryIndex();
    };
    windowObject.history = {
        get length() { return historyLength; },
        get state() { return historyState; },
        pushState(state, _title, url) {
            if (arguments.length < 2) throw new TypeError('pushState requires two arguments');
            historyString(_title);
            updateHistoryState(state, url == null ? null : historyString(url), false);
        },
        replaceState(state, _title, url) {
            if (arguments.length < 2) throw new TypeError('replaceState requires two arguments');
            historyString(_title);
            updateHistoryState(state, url == null ? null : historyString(url), true);
        },
        back() { host('historyTraverse', -1); },
        forward() { host('historyTraverse', 1); },
        go(delta = 0) { host('historyTraverse', (+delta) | 0); }
    };
    // The shell owns tab-wide history. Seed the new Document before running author scripts.
    windowObject.__setHistoryMetrics = (length, index, serialized = null) => {
        const state = serialized === null ? null : deserializeHistoryState(String(serialized));
        historyLength = Number(length);
        historyIndex = Number(index);
        historyState = state;
    };
    // Called only when the native session-history traversal activates another entry for
    // this Document. The URL and state are observable before popstate; hashchange is a task.
    windowObject.__applyHistoryTraversal = (url, serialized, length, index) => {
        const oldURL = currentUrl;
        const newURL = String(url);
        const state = serialized === null ? null : deserializeHistoryState(String(serialized));
        currentUrl = newURL;
        historyState = state;
        historyLength = Number(length);
        historyIndex = Number(index);
        firePopState();
        const fragment = value => {
            const marker = value.indexOf('#');
            return marker < 0 ? null : value.slice(marker + 1);
        };
        if (fragment(oldURL) !== fragment(newURL)) {
            queueTimer(() => windowObject.dispatchEvent(markTrusted(
                new HashChangeEvent('hashchange', { oldURL, newURL })
            )), 0, false, [], 'history hashchange');
        }
    };

    const navigatorUserAgent = host('userAgent');
    const geckoCompatibility = /\bFirefox\/\d/.test(navigatorUserAgent);
    windowObject.navigator = {
        userAgent: navigatorUserAgent,
        appCodeName: 'Mozilla',
        appName: 'Netscape',
        // HTML's legacy value differs between Gecko and Chrome compatibility.
        appVersion: navigatorUserAgent.startsWith('Mozilla/5.0 (')
            ? (geckoCompatibility ? '5.0 (Windows)' : navigatorUserAgent.slice('Mozilla/'.length)) : '',
        platform: 'Win32',
        product: 'Gecko',
        productSub: geckoCompatibility ? '20100101' : '20030107',
        vendor: geckoCompatibility ? '' : navigatorUserAgent.includes('Chrome/') ? 'Google Inc.' : '',
        vendorSub: '',
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
        javaEnabled() { return false; }
    };
    if (geckoCompatibility) {
        windowObject.navigator.oscpu = 'Windows NT 10.0; Win64; x64';
        windowObject.navigator.taintEnabled = () => false;
    }
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
            const contentSupported = source => !source ||
                (snapshot.type === 'media-source'
                    ? windowObject.MediaSource.isTypeSupported(source.contentType)
                    : supportedMediaType(source.contentType) !== '');
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
    const [initialViewportWidth = 1280, initialViewportHeight = 720, initialDeviceScale = 1,
        initialLayoutViewportWidth = initialViewportWidth,
        initialLayoutViewportHeight = initialViewportHeight] = host('viewportMetrics');
    let layoutViewportWidth = initialLayoutViewportWidth;
    let layoutViewportHeight = initialLayoutViewportHeight;
    const exposedViewportWidth = Math.round(initialViewportWidth);
    const exposedViewportHeight = Math.round(initialViewportHeight);
    windowObject.screen = {
        width: exposedViewportWidth, height: exposedViewportHeight,
        availWidth: exposedViewportWidth, availHeight: exposedViewportHeight,
        colorDepth: 24, pixelDepth: 24
    };
    windowObject.innerWidth = exposedViewportWidth;
    windowObject.innerHeight = exposedViewportHeight;
    windowObject.devicePixelRatio = initialDeviceScale;

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
