    // Media state changes only after the renderer acknowledges work by the contained media worker.
    const timeRangesConstructionToken = {};
    class TimeRanges {
        constructor(token, ranges = []) {
            if (token !== timeRangesConstructionToken) throw new TypeError('Illegal constructor');
            this.__ranges = ranges;
        }
        get length() { return this.__ranges.length; }
        start(index) { return this.__range(index)[0]; }
        end(index) { return this.__range(index)[1]; }
        __range(index) {
            index = Number(index);
            if (!Number.isInteger(index) || index < 0 || index >= this.__ranges.length)
                throw new DOMException('The index is not in the allowed range', 'IndexSizeError');
            return this.__ranges[index];
        }
    }
    const emptyTimeRanges = () => new TimeRanges(timeRangesConstructionToken);

    class MediaError {
        constructor(code, message = '') {
            this.code = Number(code) || 0;
            this.message = String(message);
        }
    }
    for (const [name, value] of Object.entries({
        MEDIA_ERR_ABORTED: 1,
        MEDIA_ERR_NETWORK: 2,
        MEDIA_ERR_DECODE: 3,
        MEDIA_ERR_SRC_NOT_SUPPORTED: 4
    })) {
        Object.defineProperty(MediaError, name, { enumerable: true, value });
        Object.defineProperty(MediaError.prototype, name, { enumerable: true, value });
    }

    const mediaStates = new WeakMap();
    let nextMediaRequest = 1;
    const pendingMediaRequests = new Map();
    const effectiveVolumeMillis = state => state.muted ? 0 : Math.round(state.volume * 1000);
    const mediaCommand = (element, requestId, command, ...args) =>
        host('mediaRequest', element.__id, requestId, command, ...args);
    const supportedMediaType = type => {
        const source = String(type).trim().toLowerCase();
        if (!source) return '';
        const [essence, ...parameters] = source.split(';').map(part => part.trim());
        if (essence !== 'video/mp4' && essence !== 'audio/mp4' && essence !== 'application/mp4')
            return '';
        const codecsParameter = parameters.find(parameter => parameter.startsWith('codecs='));
        if (!codecsParameter) return 'maybe';
        const codecs = codecsParameter.slice(codecsParameter.indexOf('=') + 1)
            .replace(/^['\"]|['\"]$/g, '').split(',').map(codec => codec.trim());
        if (!codecs.length || codecs.some(codec => !/^(avc1\.|mp4a\.40\.2$)/.test(codec))) return '';
        return 'probably';
    };
    const mediaStateFor = element => {
        let state = mediaStates.get(element);
        if (!state) {
            state = {
                networkState: 0,
                readyState: 0,
                error: null,
                currentSrc: '',
                duration: NaN,
                currentTime: 0,
                defaultPlaybackRate: 1,
                playbackRate: 1,
                volume: 1,
                muted: element.hasAttribute('muted'),
                paused: true,
                ended: false,
                seeking: false,
                preservesPitch: true,
                srcObject: null,
                buffered: emptyTimeRanges(),
                seekable: emptyTimeRanges(),
                played: emptyTimeRanges(),
                videoWidth: 0,
                videoHeight: 0
            };
            mediaStates.set(element, state);
        }
        return state;
    };
    const reflectBoolean = (prototype, property, attribute = property.toLowerCase()) =>
        Object.defineProperty(prototype, property, {
            configurable: true, enumerable: true,
            get() { return this.hasAttribute(attribute); },
            set(value) { this.toggleAttribute(attribute, !!value); }
        });
    const reflectString = (prototype, property, attribute = property.toLowerCase()) =>
        Object.defineProperty(prototype, property, {
            configurable: true, enumerable: true,
            get() { return this.getAttribute(attribute) || ''; },
            set(value) { this.setAttribute(attribute, value); }
        });

    class HTMLMediaElement extends HTMLElement {
        constructor(id, ...metadata) {
            if (id === undefined) throw new TypeError('Illegal constructor');
            super(id, ...metadata);
            mediaStateFor(this);
        }
        get error() { return mediaStateFor(this).error; }
        get networkState() { return mediaStateFor(this).networkState; }
        get readyState() { return mediaStateFor(this).readyState; }
        get currentSrc() { return mediaStateFor(this).currentSrc; }
        get duration() { return mediaStateFor(this).duration; }
        get paused() { return mediaStateFor(this).paused; }
        get ended() { return mediaStateFor(this).ended; }
        get seeking() { return mediaStateFor(this).seeking; }
        get buffered() { return mediaStateFor(this).buffered; }
        get seekable() { return mediaStateFor(this).seekable; }
        get played() { return mediaStateFor(this).played; }
        get currentTime() { return mediaStateFor(this).currentTime; }
        set currentTime(value) {
            value = Number(value);
            if (!Number.isFinite(value)) throw new TypeError('currentTime must be finite');
            const state = mediaStateFor(this);
            value = Math.max(0, Number.isFinite(state.duration) ? Math.min(value, state.duration) : value);
            if (state.readyState === HTMLMediaElement.HAVE_NOTHING) {
                state.currentTime = value;
                return;
            }
            state.seeking = true;
            state.currentTime = value;
            this.dispatchEvent(new Event('seeking'));
            mediaCommand(this, 0, 'seek', value);
        }
        get defaultPlaybackRate() { return mediaStateFor(this).defaultPlaybackRate; }
        set defaultPlaybackRate(value) {
            value = Number(value);
            if (!Number.isFinite(value) || value === 0)
                throw new DOMException('Unsupported playback rate', 'NotSupportedError');
            mediaStateFor(this).defaultPlaybackRate = value;
            this.dispatchEvent(new Event('ratechange'));
        }
        get playbackRate() { return mediaStateFor(this).playbackRate; }
        set playbackRate(value) {
            value = Number(value);
            if (!Number.isFinite(value) || value === 0)
                throw new DOMException('Unsupported playback rate', 'NotSupportedError');
            mediaStateFor(this).playbackRate = value;
            this.dispatchEvent(new Event('ratechange'));
        }
        get volume() { return mediaStateFor(this).volume; }
        set volume(value) {
            value = Number(value);
            if (!Number.isFinite(value) || value < 0 || value > 1)
                throw new DOMException('Volume must be between zero and one', 'IndexSizeError');
            const state = mediaStateFor(this);
            if (state.volume === value) return;
            state.volume = value;
            this.dispatchEvent(new Event('volumechange'));
            mediaCommand(this, 0, 'configure', effectiveVolumeMillis(state));
        }
        get muted() { return mediaStateFor(this).muted; }
        set muted(value) {
            const state = mediaStateFor(this);
            value = !!value;
            if (state.muted === value) return;
            state.muted = value;
            this.dispatchEvent(new Event('volumechange'));
            mediaCommand(this, 0, 'configure', effectiveVolumeMillis(state));
        }
        get preservesPitch() { return mediaStateFor(this).preservesPitch; }
        set preservesPitch(value) { mediaStateFor(this).preservesPitch = !!value; }
        get mediaKeys() { return null; }
        setMediaKeys() {
            return Promise.reject(new DOMException(
                'Encrypted media playback is not supported',
                'NotSupportedError'
            ));
        }
        get srcObject() { return mediaStateFor(this).srcObject; }
        set srcObject(value) {
            if (value !== null) throw new TypeError('MediaStream playback is not supported');
            mediaStateFor(this).srcObject = null;
        }
        load() {
            const state = mediaStateFor(this);
            const hadResource = state.networkState !== HTMLMediaElement.NETWORK_EMPTY;
            state.networkState = HTMLMediaElement.NETWORK_EMPTY;
            state.readyState = HTMLMediaElement.HAVE_NOTHING;
            state.error = null;
            state.currentSrc = '';
            state.duration = NaN;
            state.currentTime = 0;
            state.paused = true;
            state.ended = false;
            state.seeking = false;
            state.buffered = emptyTimeRanges();
            state.seekable = emptyTimeRanges();
            state.played = emptyTimeRanges();
            mediaCommand(this, 0, 'reset');
            if (hadResource) this.dispatchEvent(new Event('emptied'));
        }
        play() {
            const state = mediaStateFor(this);
            const requestId = nextMediaRequest++;
            return new Promise((resolve, reject) => {
                pendingMediaRequests.set(requestId, { element: this, resolve, reject });
                const volumeMillis = effectiveVolumeMillis(state);
                if (!prepareMediaSourcePlayback(this, requestId, volumeMillis))
                    mediaCommand(this, requestId, 'playback', true, volumeMillis);
            });
        }
        pause() {
            const state = mediaStateFor(this);
            mediaCommand(this, 0, 'playback', false, effectiveVolumeMillis(state));
        }
        fastSeek(time) { this.currentTime = time; }
        canPlayType(type) { return supportedMediaType(type); }
        getStartDate() { return new Date(NaN); }
    }
    installEventHandlerAttributes(HTMLMediaElement.prototype);
    for (const [name, value] of Object.entries({
        NETWORK_EMPTY: 0,
        NETWORK_IDLE: 1,
        NETWORK_LOADING: 2,
        NETWORK_NO_SOURCE: 3,
        HAVE_NOTHING: 0,
        HAVE_METADATA: 1,
        HAVE_CURRENT_DATA: 2,
        HAVE_FUTURE_DATA: 3,
        HAVE_ENOUGH_DATA: 4
    })) {
        Object.defineProperty(HTMLMediaElement, name, { enumerable: true, value });
        Object.defineProperty(HTMLMediaElement.prototype, name, { enumerable: true, value });
    }
    reflectBoolean(HTMLMediaElement.prototype, 'autoplay');
    reflectBoolean(HTMLMediaElement.prototype, 'controls');
    reflectBoolean(HTMLMediaElement.prototype, 'loop');
    reflectBoolean(HTMLMediaElement.prototype, 'defaultMuted', 'muted');
    reflectBoolean(HTMLMediaElement.prototype, 'disableRemotePlayback', 'disableremoteplayback');
    reflectString(HTMLMediaElement.prototype, 'preload');
    Object.defineProperty(HTMLMediaElement.prototype, 'crossOrigin', {
        configurable: true, enumerable: true,
        get() { return this.hasAttribute('crossorigin') ? this.getAttribute('crossorigin') : null; },
        set(value) {
            if (value == null) this.removeAttribute('crossorigin');
            else this.setAttribute('crossorigin', value);
        }
    });

    class HTMLVideoElement extends HTMLMediaElement {
        get videoWidth() { return mediaStateFor(this).videoWidth; }
        get videoHeight() { return mediaStateFor(this).videoHeight; }
    }
    reflectString(HTMLVideoElement.prototype, 'poster');
    reflectBoolean(HTMLVideoElement.prototype, 'playsInline', 'playsinline');

    class HTMLAudioElement extends HTMLMediaElement {}

    reflectString(HTMLSourceElement.prototype, 'type');

    const applyMediaResponse = input => {
        const element = wrap(Number(input.target) || 0);
        if (!(element instanceof HTMLMediaElement)) return false;
        const state = mediaStateFor(element);
        const requestId = Number(input.requestId) || 0;
        const pending = requestId ? pendingMediaRequests.get(requestId) : null;
        if (requestId) pendingMediaRequests.delete(requestId);
        switch (input.disposition) {
            case 'loaded':
                state.networkState = HTMLMediaElement.NETWORK_IDLE;
                state.readyState = HTMLMediaElement.HAVE_CURRENT_DATA;
                state.currentSrc = element.src;
                state.duration = Number(input.duration);
                state.videoWidth = Number(input.width) || 0;
                state.videoHeight = Number(input.height) || 0;
                state.buffered = new TimeRanges(timeRangesConstructionToken, [[0, state.duration]]);
                state.seekable = new TimeRanges(timeRangesConstructionToken, [[0, state.duration]]);
                notifyMediaSourceLoaded(element, state.duration);
                element.dispatchEvent(markTrusted(new Event('durationchange')));
                element.dispatchEvent(markTrusted(new Event('loadedmetadata')));
                element.dispatchEvent(markTrusted(new Event('loadeddata')));
                element.dispatchEvent(markTrusted(new Event('canplay')));
                if (element.autoplay) element.play().catch(() => {});
                return true;
            case 'playing':
                state.paused = false;
                state.ended = false;
                pending?.resolve();
                element.dispatchEvent(markTrusted(new Event('play')));
                element.dispatchEvent(markTrusted(new Event('playing')));
                return true;
            case 'paused':
                if (!state.paused) {
                    state.paused = true;
                    element.dispatchEvent(markTrusted(new Event('pause')));
                }
                return true;
            case 'time':
                state.currentTime = Math.max(0, Number(input.currentTime) || 0);
                state.played = new TimeRanges(timeRangesConstructionToken, [[0, state.currentTime]]);
                element.dispatchEvent(markTrusted(new Event('timeupdate')));
                return true;
            case 'seeked':
                state.currentTime = Math.max(0, Number(input.currentTime) || 0);
                state.seeking = false;
                element.dispatchEvent(markTrusted(new Event('timeupdate')));
                element.dispatchEvent(markTrusted(new Event('seeked')));
                return true;
            case 'configured':
            case 'reset':
            case 'committed':
                return true;
            case 'media-error':
                notifyMediaSourceError(element);
                return false;
            case 'ended':
                state.currentTime = Number.isFinite(state.duration) ? state.duration : state.currentTime;
                state.paused = true;
                state.ended = true;
                element.dispatchEvent(markTrusted(new Event('timeupdate')));
                element.dispatchEvent(markTrusted(new Event('ended')));
                return true;
            default:
                pending?.reject(new DOMException('Media playback is unavailable', 'NotSupportedError'));
                return false;
        }
    };
