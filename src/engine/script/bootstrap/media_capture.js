// A MediaStream is created only after the browser has granted and started contained capture.
// Device identity and the private capture broker ID never enter this realm.
(() => {
    'use strict';
    if (!navigator.mediaDevices) {
        // The embedder captures these callbacks before author script even in an insecure
        // realm. Such a realm cannot request capture, so every delivery is a no-op.
        globalThis.__receiveMediaCaptureUpdate = () => {};
        globalThis.__receiveMediaCaptureFrame = () => {};
        globalThis.__receiveMediaCaptureAudioFrame = () => {};
        return;
    }
    const native = globalThis.__hostCall;
    const trackToken = Symbol('granted capture track');
    const trackState = new WeakMap();
    const streamState = new WeakMap();
    const pending = new Map();
    const active = new Map();
    const captureTracks = new Map();
    const streamsByCapture = new Map();
    let nextStreamId = 1;

    const streamsForTrack = track => [...trackState.get(track).streams];
    const clearCapturedAudio = track => {
        for (const stream of streamsByCapture.get(trackState.get(track).requestId) || [])
            globalThis.__webAudioCaptureStopped?.(stream);
    };

    const normalizeConstraint = (value, kind) => {
        if (value === undefined || value === false) return false;
        if (value === true) return true;
        if (value === null || typeof value !== 'object')
            throw new TypeError(`${kind} must be a boolean or constraint set`);
        // The native backend currently selects the system default device. Reject mandatory
        // constraints it cannot check rather than silently returning a nonconforming track.
        for (const [name, setting] of Object.entries(value)) {
            if (name === 'advanced' ||
                (setting && typeof setting === 'object' &&
                    ('exact' in setting || 'min' in setting || 'max' in setting))) {
                const error = new DOMException(`Unsupported mandatory ${name} constraint`,
                    'OverconstrainedError');
                Object.defineProperty(error, 'constraint', { value: name });
                throw error;
            }
        }
        return true;
    };

    class MediaStreamTrack extends EventTarget {
        constructor(token, requestId, kind, trackId) {
            super();
            if (token !== trackToken) throw new TypeError('Illegal constructor');
            trackState.set(this, {
                requestId, trackId, kind, id: `breeze-track-${requestId}-${trackId}`,
                enabled: true, muted: false, ended: false, streams: new Set()
            });
        }
        get kind() { return trackState.get(this).kind; }
        get id() { return trackState.get(this).id; }
        get label() { return ''; }
        get enabled() { return trackState.get(this).enabled; }
        set enabled(value) {
            const state = trackState.get(this);
            if (state.ended) return;
            state.enabled = !!value;
            if (state.kind === 'audio' && !state.enabled) clearCapturedAudio(this);
            native('mediaCaptureEnable', state.requestId, state.trackId, state.enabled);
        }
        get muted() { return trackState.get(this).muted; }
        get readyState() { return trackState.get(this).ended ? 'ended' : 'live'; }
        get contentHint() { return ''; }
        set contentHint(_) {}
        getCapabilities() { return {}; }
        getConstraints() { return {}; }
        getSettings() {
            const state = trackState.get(this);
            return state.kind === 'video' ? { width: state.width || 0, height: state.height || 0 } : {};
        }
        applyConstraints(constraints = {}) {
            if (constraints && Object.keys(constraints).length)
                return Promise.reject(new DOMException('Capture constraints are not adjustable',
                    'OverconstrainedError'));
            return Promise.resolve();
        }
        clone() {
            throw new DOMException('Capture track cloning is not supported', 'NotSupportedError');
        }
        stop() {
            const state = trackState.get(this);
            if (state.ended) return;
            state.ended = true;
            if (state.kind === 'audio') clearCapturedAudio(this);
            native('mediaCaptureStop', state.requestId, state.trackId);
            for (const stream of streamsForTrack(this)) stream.__trackStopped();
        }
    }

    class MediaStream extends EventTarget {
        constructor(tracks = []) {
            super();
            if (tracks instanceof MediaStream) tracks = tracks.getTracks();
            if (!Array.isArray(tracks) || !tracks.every(track => trackState.has(track)))
                throw new TypeError('MediaStream requires capture tracks');
            const requestIds = new Set(tracks.map(track => trackState.get(track).requestId));
            if (requestIds.size > 1)
                throw new DOMException('Tracks belong to different capture sessions', 'NotSupportedError');
            const requestId = tracks.length ? trackState.get(tracks[0]).requestId : 0;
            streamState.set(this, {
                id: `breeze-stream-${nextStreamId++}`,
                requestId, tracks: [...tracks], elements: new Map(),
                wasActive: tracks.some(track => track.readyState === 'live'),
            });
            for (const track of tracks) trackState.get(track).streams.add(this);
            if (requestId) {
                if (!streamsByCapture.has(requestId)) streamsByCapture.set(requestId, new Set());
                streamsByCapture.get(requestId).add(this);
            }
        }
        get id() { return streamState.get(this).id; }
        get active() { return this.getTracks().some(track => track.readyState === 'live'); }
        getTracks() { return [...streamState.get(this).tracks]; }
        getAudioTracks() { return this.getTracks().filter(track => track.kind === 'audio'); }
        getVideoTracks() { return this.getTracks().filter(track => track.kind === 'video'); }
        getTrackById(id) { return this.getTracks().find(track => track.id === String(id)) || null; }
        addTrack(track) {
            if (!trackState.has(track)) throw new TypeError('Expected MediaStreamTrack');
            const state = streamState.get(this);
            if (state.requestId && state.requestId !== trackState.get(track).requestId)
                throw new DOMException('Tracks belong to different capture sessions', 'NotSupportedError');
            if (state.tracks.includes(track)) return;
            state.requestId = trackState.get(track).requestId;
            state.tracks.push(track);
            trackState.get(track).streams.add(this);
            if (!streamsByCapture.has(state.requestId))
                streamsByCapture.set(state.requestId, new Set());
            streamsByCapture.get(state.requestId).add(this);
            this.dispatchEvent(new Event('addtrack'));
            if (!state.wasActive && this.active) this.dispatchEvent(new Event('active'));
            state.wasActive = this.active;
        }
        removeTrack(track) {
            const state = streamState.get(this);
            const index = state.tracks.indexOf(track);
            if (index < 0) return;
            state.tracks.splice(index, 1);
            trackState.get(track).streams.delete(this);
            this.dispatchEvent(new Event('removetrack'));
            this.__trackStopped();
        }
        clone() {
            throw new DOMException('Capture stream cloning is not supported', 'NotSupportedError');
        }
        __trackStopped() {
            const state = streamState.get(this);
            const active = this.active;
            if (state.wasActive && !active) this.dispatchEvent(new Event('inactive'));
            state.wasActive = active;
        }
    }

    MediaDevices.prototype.getUserMedia = function(constraints) {
        if (this !== navigator.mediaDevices) throw new TypeError('Illegal invocation');
        return new Promise((resolve, reject) => {
            try {
                if (!constraints || typeof constraints !== 'object')
                    throw new TypeError('getUserMedia requires a constraints object');
                const microphone = normalizeConstraint(constraints.audio, 'audio');
                const camera = normalizeConstraint(constraints.video, 'video');
                if (!microphone && !camera)
                    throw new TypeError('At least one media track must be requested');
                const id = Number(native('mediaCaptureStart', microphone, camera));
                pending.set(id, { resolve, reject });
            } catch (error) { reject(error); }
        });
    };

    globalThis.MediaStream = MediaStream;
    globalThis.MediaStreamTrack = MediaStreamTrack;
    globalThis.__isCaptureStream = value => streamState.has(value);
    globalThis.__attachCaptureStream = (element, node) => {
        const stream = element.srcObject;
        const state = streamState.get(stream);
        if (!state?.requestId) return;
        state.elements.set(node, element);
        native('mediaCaptureAttach', state.requestId, node);
    };
    globalThis.__detachCaptureStream = (element, node) => {
        const stream = element.srcObject;
        const state = streamState.get(stream);
        if (!state) return;
        state.elements.delete(node);
        native('mediaCaptureDetach', state.requestId, node);
    };
    globalThis.__receiveMediaCaptureUpdate = payload => {
        const update = JSON.parse(String(payload));
        const id = Number(update.id);
        if (update.kind === 'started') {
            const item = pending.get(id);
            if (!item) return;
            pending.delete(id);
            const tracks = [];
            if (update.camera) tracks.push(new MediaStreamTrack(trackToken, id, 'video', 1));
            if (update.microphone) tracks.push(new MediaStreamTrack(trackToken, id, 'audio', 2));
            const stream = new MediaStream(tracks);
            active.set(id, stream);
            captureTracks.set(id, tracks);
            item.resolve(stream);
            return;
        }
        if (update.kind === 'error') {
            const item = pending.get(id);
            pending.delete(id);
            item?.reject(new DOMException('Media capture failed', update.error));
        }
        const stream = active.get(id);
        const tracks = captureTracks.get(id);
        if (!stream || !tracks) return;
        const ended = update.kind === 'trackEnded' ? update.trackId :
            update.kind === 'ended' || update.kind === 'error' ? 0 : -1;
        if (ended === -1) return;
        for (const track of tracks) {
            const state = trackState.get(track);
            if (ended && state.trackId !== ended || state.ended) continue;
            state.ended = true;
            track.dispatchEvent(new Event('ended'));
            if (state.kind === 'audio') clearCapturedAudio(track);
        }
        for (const attached of streamsByCapture.get(id) || []) attached.__trackStopped();
        if (tracks.every(track => track.readyState === 'ended')) {
            active.delete(id);
            captureTracks.delete(id);
            streamsByCapture.delete(id);
        }
    };
    globalThis.__receiveMediaCaptureFrame = payload => {
        const frame = JSON.parse(String(payload));
        const stream = active.get(Number(frame.id));
        if (!stream) return;
        const track = captureTracks.get(Number(frame.id))?.find(track => track.kind === 'video');
        if (!track || track.readyState !== 'live') return;
        const state = trackState.get(track);
        state.width = Number(frame.width) || 0;
        state.height = Number(frame.height) || 0;
        if (!state.enabled) return;
        for (const attached of streamsByCapture.get(Number(frame.id)) || []) {
            if (!attached.getVideoTracks().includes(track)) continue;
            for (const element of streamState.get(attached).elements.values())
                globalThis.__receiveCapturedMediaFrame?.(element, frame);
        }
    };
    globalThis.__receiveMediaCaptureAudioFrame = (id, sequence, timestamp100ns,
        sampleRate, channels, frames, pcmBytes) => {
        const stream = active.get(Number(id));
        const track = captureTracks.get(Number(id))?.find(track => track.kind === 'audio');
        if (!track || track.readyState !== 'live' || !(pcmBytes instanceof Uint8Array)) return;
        const bytes = Number(frames) * Number(channels) * 2;
        if (pcmBytes.length !== bytes || bytes > 3840 || bytes <= 0) return;
        const samples = track.enabled ? pcmBytes : new Uint8Array(bytes);
        for (const attached of streamsByCapture.get(Number(id)) || []) {
            if (attached === stream || attached.getAudioTracks().includes(track))
                globalThis.__webAudioCaptureFrame?.(attached, sequence, timestamp100ns,
                    sampleRate, channels, frames, samples);
        }
    };
})();
