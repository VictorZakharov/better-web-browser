    let historyLength = 1;
    windowObject.history = {
        get length() { return historyLength; },
        state: null,
        pushState(state, _title, url) {
            this.state = state;
            currentUrl = host('historyUpdate', url == null ? currentUrl : String(url), false);
            historyLength++;
        },
        replaceState(state, _title, url) {
            this.state = state;
            currentUrl = host('historyUpdate', url == null ? currentUrl : String(url), true);
        },
        back() {}, forward() {}, go() {}
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
        sendBeacon(url) { host('console', 'beacon', String(url)); return false; },
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
