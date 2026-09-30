// MediaStream Recording: one captured audio track, encoded incrementally.
// See https://w3c.github.io/mediacapture-record/, RFC 9639, and RFC 7845.
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    const queueTask = globalThis.setTimeout.bind(globalThis);
    const markTrusted = globalThis.__markTrustedEvent;
    const BlobCtor = globalThis.Blob;
    const makeBlob = BlobCtor.__fromOwnedBytes;
    const CaptureStream = globalThis.MediaStream;
    const mimeFor = kind => kind === 'opus' ? 'audio/ogg;codecs=opus' : 'audio/flac';
    const MAX_BLOB_BYTES = 16 * 1024 * 1024;
    const MIN_TIMESLICE = 100;
    const CAPTURE_TICKS_PER_SECOND = 10_000_000;
    const states = new WeakMap();
    const handlers = new WeakMap();
    const byTrack = new WeakMap();

    const recorderState = recorder => {
        const state = states.get(recorder);
        if (!state) throw new TypeError('Invalid MediaRecorder receiver');
        return state;
    };
    const recordingKind = value => native('mediaRecorderType', `${value}`);
    const isTypeSupported = value => recordingKind(value) !== '';
    const unsignedLong = value => {
        const number = +value;
        if (!Number.isFinite(number) || number === 0) return 0;
        return ((Math.trunc(number) % 0x100000000) + 0x100000000) % 0x100000000;
    };
    const finiteDouble = value => {
        const number = +value;
        if (!Number.isFinite(number)) throw new TypeError('Recording duration must be finite');
        return number;
    };
    const optionsDictionary = options => {
        if (options == null) options = {};
        else if (typeof options !== 'object' && typeof options !== 'function')
            throw new TypeError('Recorder options must be a dictionary');
        // WebIDL reads dictionary members once, converting each in name order.
        const modeValue = options.audioBitrateMode;
        const mode = modeValue === undefined ? 'variable' : `${modeValue}`;
        if (mode !== 'variable' && mode !== 'constant') throw new TypeError('Invalid audio bitrate mode');
        const audioValue = options.audioBitsPerSecond;
        const audio = audioValue === undefined ? undefined : unsignedLong(audioValue);
        const aggregateValue = options.bitsPerSecond;
        const aggregate = aggregateValue === undefined ? undefined : unsignedLong(aggregateValue);
        const mimeValue = options.mimeType;
        const mime = mimeValue === undefined ? '' : `${mimeValue}`;
        const videoValue = options.videoBitsPerSecond;
        const video = videoValue === undefined ? 0 : unsignedLong(videoValue);
        const countValue = options.videoKeyFrameIntervalCount;
        const count = countValue === undefined ? undefined : unsignedLong(countValue);
        const durationValue = options.videoKeyFrameIntervalDuration;
        const duration = durationValue === undefined ? undefined : finiteDouble(durationValue);
        return {mode, audio, aggregate, mime, video, count, duration};
    };
    const queueEvent = (recorder, event) => queueTask(() =>
        recorder.dispatchEvent(markTrusted(event)), 0);
    const removeTrackListeners = session => {
        session.stream.removeEventListener('addtrack', session.trackChanged);
        session.stream.removeEventListener('removetrack', session.trackChanged);
        session.stream.removeEventListener('inactive', session.inactive);
        const sessions = byTrack.get(session.track);
        sessions?.delete(session);
        if (sessions?.size === 0) byTrack.delete(session.track);
    };
    const appendOutput = (session, bytes) => {
        if (!(bytes instanceof Uint8Array)) throw new Error('Audio encoder returned no bytes');
        if (!bytes.byteLength) return;
        if (bytes.byteLength > MAX_BLOB_BYTES - session.size)
            throw new Error('Audio recording data limit exceeded');
        session.chunks.push(bytes);
        session.size += bytes.byteLength;
    };
    const appendCapture = (session, timestamp, sampleRate, channels, frames, pcm) => {
        if (session.firstTimestamp === null) session.firstTimestamp = timestamp;
        if (session.chunkTimestamp === null) session.chunkTimestamp = timestamp;
        session.lastTimestamp = timestamp;
        appendOutput(session, native('mediaRecorderEncode', session.id,
            sampleRate, channels, pcm));
        if (session.slice !== null) {
            session.sliceElapsed += frames * 1000 / sampleRate;
            if (!session.sliceQueued && session.sliceElapsed >= session.slice) {
                session.sliceQueued = true;
                queueTask(() => {
                    session.sliceQueued = false;
                    if (session.finished) return;
                    session.sliceElapsed = 0;
                    emitData(session.recorder, session);
                }, 0);
            }
        }
    };
    const fillCaptureGap = (session, sequence, timestamp, sampleRate, channels) => {
        if (!Number.isSafeInteger(sequence) || sequence < 1 ||
            !Number.isSafeInteger(timestamp) || timestamp < 0)
            throw new Error('Invalid audio capture sequence or timestamp');
        const previous = session.lastPacket;
        if (!previous) return;
        if (sequence <= previous.sequence || timestamp <= previous.timestamp)
            throw new Error('Audio capture packets arrived out of order');
        if (sampleRate !== previous.sampleRate || channels !== previous.channels)
            throw new Error('Audio capture format changed during recording');
        // Capture timestamps are wall-clock observations and can jitter even
        // when every PCM packet arrived. Sequence continuity wins in that case.
        if (sequence === previous.sequence + 1) return;
        const elapsedFrames = Math.round((timestamp - previous.timestamp) *
            sampleRate / CAPTURE_TICKS_PER_SECOND);
        const missingFrames = elapsedFrames - previous.frames;
        if (missingFrames < -1 || sequence > previous.sequence + 1 && missingFrames <= 0)
            throw new Error('Audio capture timestamps cannot represent a packet gap');
        if (missingFrames <= 0) return;
        if (missingFrames > sampleRate)
            throw new Error('Audio capture gap exceeds the one-second recording limit');
        const gapStart = Math.round(previous.timestamp + previous.frames *
            CAPTURE_TICKS_PER_SECOND / sampleRate);
        for (let remaining = missingFrames, filled = 0; remaining > 0;) {
            const frames = Math.min(960, remaining);
            const gapTimestamp = Math.round(gapStart + filled *
                CAPTURE_TICKS_PER_SECOND / sampleRate);
            appendCapture(session, gapTimestamp, sampleRate, channels, frames,
                new Uint8Array(frames * channels * 2));
            remaining -= frames;
            filled += frames;
        }
    };

    class BlobEvent extends Event {
        constructor(type, init) {
            if (!init || !(init.data instanceof BlobCtor))
                throw new TypeError('BlobEvent requires a Blob');
            super(type, init);
            Object.defineProperties(this, {
                data: { value: init.data, enumerable: true },
                timecode: { value: Number(init.timecode ?? 0), enumerable: true }
            });
        }
    }

    const emitData = (recorder, session) => {
        const blob = makeBlob(session.chunks, session.mimeType);
        session.chunks = [];
        session.size = 0;
        const timecode = session.emitted++ === 0 ? 0 :
            Math.max(0, ((session.chunkTimestamp ?? session.lastTimestamp ??
                session.firstTimestamp ?? 0) - (session.firstTimestamp ?? 0)) / 10000);
        session.chunkTimestamp = null;
        recorder.dispatchEvent(markTrusted(new BlobEvent('dataavailable', { data: blob, timecode })));
    };
    const retireNative = session => {
        if (session.nativeClosed) return null;
        session.nativeClosed = true;
        try {
            appendOutput(session, native('mediaRecorderFinish', session.id));
        } catch (error) {
            native('mediaRecorderCancel', session.id);
            return new DOMException(String(error?.message || error), 'UnknownError');
        }
        return null;
    };
    const finish = (recorder, session, failure = null) => {
        if (session.finished) return;
        session.finished = true;
        session.collecting = false;
        removeTrackListeners(session);
        failure ||= retireNative(session);
        if (failure)
            recorder.dispatchEvent(markTrusted(new ErrorEvent('error', {
                error: failure, message: failure.message
            })));
        emitData(recorder, session);
        recorder.dispatchEvent(markTrusted(new Event('stop')));
    };
    const fail = (recorder, session, name, message) => {
        if (session.closing) return;
        session.closing = true;
        session.collecting = false;
        const state = recorderState(recorder);
        if (state.session === session) {
            state.state = 'inactive';
            state.mimeType = state.constrainedMimeType;
        }
        retireNative(session);
        queueTask(() => finish(recorder, session, new DOMException(message, name)), 0);
    };

    class MediaRecorder extends EventTarget {
        constructor(stream, options = {}) {
            super();
            if (typeof CaptureStream !== 'function' || !(stream instanceof CaptureStream))
                throw new TypeError('MediaRecorder requires a MediaStream');
            const dictionary = optionsDictionary(options);
            const mimeType = dictionary.mime;
            const kind = recordingKind(mimeType);
            if (!kind)
                throw new DOMException('Unsupported recording MIME type', 'NotSupportedError');
            const requestedRate = dictionary.aggregate ?? dictionary.audio ?? 128000;
            states.set(this, { stream, constrainedMimeType: mimeType,
                mimeType, kind, state: 'inactive', session: null,
                // Reflect the converted hint; native encoder bounds do not
                // change the constructor's WebIDL-visible requested value.
                audioBitsPerSecond: requestedRate,
                videoBitsPerSecond: dictionary.aggregate === undefined ? dictionary.video : 0,
                mode: kind === 'opus' ? dictionary.mode : 'variable',
                conflictingKeyFrames: dictionary.count !== undefined && dictionary.duration !== undefined });
        }
        static isTypeSupported(type) {
            if (arguments.length === 0)
                throw new TypeError('MediaRecorder.isTypeSupported requires a MIME type');
            return isTypeSupported(type);
        }
        get stream() { return recorderState(this).stream; }
        get mimeType() { return recorderState(this).mimeType; }
        get state() { return recorderState(this).state; }
        get videoBitsPerSecond() { return recorderState(this).videoBitsPerSecond; }
        get audioBitsPerSecond() { return recorderState(this).audioBitsPerSecond; }
        get audioBitrateMode() { return recorderState(this).mode; }
        start(timeslice = undefined) {
            const state = recorderState(this);
            const slice = timeslice === undefined ? null :
                Math.max(MIN_TIMESLICE, unsignedLong(timeslice));
            if (state.state !== 'inactive')
                throw new DOMException('Recorder is already active', 'InvalidStateError');
            if (state.conflictingKeyFrames)
                throw new DOMException('Only one key-frame interval constraint may be specified', 'NotSupportedError');
            const tracks = state.stream.getTracks();
            if (!state.stream.active || tracks.length !== 1 || tracks[0].kind !== 'audio' ||
                tracks[0].readyState !== 'live')
                throw new DOMException('Only one live audio track can be recorded',
                    'NotSupportedError');
            const id = Number(native('mediaRecorderOpen', state.kind,
                state.audioBitsPerSecond, state.mode === 'constant'));
            const session = { id, stream: state.stream, track: tracks[0], recorder: this,
                mimeType: mimeFor(state.kind),
                chunks: [], size: 0, firstTimestamp: null, lastTimestamp: null,
                lastPacket: null, chunkTimestamp: null, emitted: 0, slice, sliceElapsed: 0,
                sliceQueued: false, collecting: true, closing: false,
                nativeClosed: false, finished: false };
            session.trackChanged = () => fail(this, session, 'InvalidModificationError',
                'MediaStream tracks changed during recording');
            session.inactive = () => { if (!session.closing) this.stop(); };
            state.stream.addEventListener('addtrack', session.trackChanged);
            state.stream.addEventListener('removetrack', session.trackChanged);
            state.stream.addEventListener('inactive', session.inactive);
            let sessions = byTrack.get(session.track);
            if (!sessions) byTrack.set(session.track, sessions = new Set());
            sessions.add(session);
            state.session = session;
            state.state = 'recording';
            queueTask(() => {
                if (session.finished) return;
                if (state.session === session && state.state !== 'inactive')
                    state.mimeType = session.mimeType;
                this.dispatchEvent(markTrusted(new Event('start')));
            }, 0);
        }
        stop() {
            const state = recorderState(this);
            if (state.state === 'inactive') return;
            const session = state.session;
            state.state = 'inactive';
            state.mimeType = state.constrainedMimeType;
            session.closing = true;
            session.collecting = false;
            const failure = retireNative(session);
            queueTask(() => finish(this, session, failure), 0);
        }
        pause() {
            const state = recorderState(this);
            if (state.state === 'inactive')
                throw new DOMException('Recorder is inactive', 'InvalidStateError');
            if (state.state === 'paused') return;
            state.state = 'paused';
            state.session.collecting = false;
            queueEvent(this, new Event('pause'));
        }
        resume() {
            const state = recorderState(this);
            if (state.state === 'inactive')
                throw new DOMException('Recorder is inactive', 'InvalidStateError');
            if (state.state === 'recording') return;
            state.state = 'recording';
            state.session.collecting = true;
            // A paused interval is deliberately omitted from the recorded media.
            state.session.lastPacket = null;
            queueEvent(this, new Event('resume'));
        }
        requestData() {
            const state = recorderState(this);
            if (state.state === 'inactive')
                throw new DOMException('Recorder is inactive', 'InvalidStateError');
            const session = state.session;
            queueTask(() => { if (!session.finished) emitData(this, session); }, 0);
        }
    }

    for (const type of ['start', 'stop', 'dataavailable', 'pause', 'resume', 'error']) {
        Object.defineProperty(MediaRecorder.prototype, 'on' + type, {
            configurable: true, enumerable: true,
            get() { return handlers.get(recorderState(this))?.[type] ?? null; },
            set(value) {
                const state = recorderState(this);
                let assigned = handlers.get(state);
                if (!assigned) handlers.set(state, assigned = {});
                if (assigned[type]) this.removeEventListener(type, assigned[type]);
                assigned[type] = typeof value === 'function' ? value : null;
                if (assigned[type]) this.addEventListener(type, assigned[type]);
            }
        });
    }

    globalThis.__installMediaRecorderCapture?.((track, sequence, timestamp,
        sampleRate, channels, frames, pcm) => {
        const sessions = byTrack.get(track);
        if (!sessions) return;
        for (const session of [...sessions]) {
            if (!session.collecting || session.finished) continue;
            try {
                // Check the actual encoder before gap synthesis can classify a
                // changed/unsupported capture rate as an unrelated sequence error.
                if (!native('mediaRecorderFormat', session.id, sampleRate, channels))
                    throw new DOMException('Capture format is unsupported or changed during recording',
                        'NotSupportedError');
                fillCaptureGap(session, sequence, timestamp, sampleRate, channels);
                appendCapture(session, timestamp, sampleRate, channels, frames, pcm);
                session.lastPacket = { sequence, timestamp, frames, sampleRate, channels };
            } catch (error) {
                fail(session.recorder, session,
                    error instanceof DOMException ? error.name : 'UnknownError',
                    String(error?.message || error));
            }
        }
    });
    delete globalThis.__installMediaRecorderCapture;
    Object.assign(globalThis, { MediaRecorder, BlobEvent });
})();
