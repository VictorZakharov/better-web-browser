    // HTML media state starts closed: capability APIs must not advertise a format until the
    // complete decode, presentation, and audio path is connected. These bindings establish the
    // standards-facing object model without inventing successful playback.
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
                muted: false,
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
            mediaStateFor(this).currentTime = Math.max(0, value);
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
        }
        get muted() { return mediaStateFor(this).muted; }
        set muted(value) {
            const state = mediaStateFor(this);
            value = !!value;
            if (state.muted === value) return;
            state.muted = value;
            this.dispatchEvent(new Event('volumechange'));
        }
        get preservesPitch() { return mediaStateFor(this).preservesPitch; }
        set preservesPitch(value) { mediaStateFor(this).preservesPitch = !!value; }
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
            if (hadResource) this.dispatchEvent(new Event('emptied'));
        }
        play() {
            return Promise.reject(new DOMException(
                'No media format has a complete playback path', 'NotSupportedError'));
        }
        pause() {
            const state = mediaStateFor(this);
            if (state.paused) return;
            state.paused = true;
            this.dispatchEvent(new Event('pause'));
        }
        fastSeek(time) { this.currentTime = time; }
        canPlayType(_type) { return ''; }
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
