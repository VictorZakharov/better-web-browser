    const cancelImageDecodes = (state, error) => {
        state.generation++;
        for (const request of state.pending) {
            if (request.timer !== null) clearTimeout(request.timer);
            request.reject(error);
        }
        state.pending.clear();
    };
    const closeImageDecoder = (state, error) => {
        if (state.closed) return;
        state.closed = true;
        cancelImageDecodes(state, error);
        if (state.timer !== null) clearTimeout(state.timer);
        state.timer = null;
        if (state.id !== null) host('imageDecoderClose', state.id);
        state.id = null;
        state.bytes = null;
        if (state.reader) {
            const reader = state.reader;
            state.reader = null;
            imageDecoderStreams.cancel(reader, error).catch(() => {}).finally(() => {
                try { imageDecoderStreams.release(reader); } catch (_) {}
            });
        }
        if (!state.trackItems.length) state.ready.reject(error);
        for (const track of state.trackItems) imageTrackState(track).selected = false;
        state.trackItems = [];
        state.selectedIndex = -1;
        if (!state.complete) state.completed.reject(error);
    };
    const failImageDecoder = (state, error) => {
        const exception = error instanceof DOMException ? error : frameError(String(error?.message ?? error), 'EncodingError');
        closeImageDecoder(state, exception);
    };
    const startImageDecoder = state => {
        if (state.closed || !state.complete || !state.bytes) return;
        try {
            state.id = host('imageDecoderStart', state.options.mime, state.bytes,
                state.options.colorSpaceConversion === 'none');
            state.bytes = null;
        } catch (error) {
            failImageDecoder(state, frameError(String(error.message), 'NotSupportedError'));
            return;
        }
        const poll = () => {
            state.timer = null;
            if (state.closed) return;
            try {
                const result = host('imageDecoderPoll', state.id);
                if (result.status === 'pending') { state.timer = setTimeout(poll, 4); return; }
                if (result.status !== 'ready') throw frameError(result.message, 'EncodingError');
                setImageTracks(state, result);
            } catch (error) { failImageDecoder(state, error); }
        };
        state.timer = setTimeout(poll, 0);
    };
    const readImageDecoderStream = async state => {
        const chunks = [];
        let length = 0;
        const reader = state.reader;
        try {
            while (!state.closed) {
                const result = await imageDecoderStreams.read(reader);
                if (state.closed) return;
                if (result.done) break;
                if (!(result.value instanceof Uint8Array))
                    throw new TypeError('Image stream chunks must be Uint8Arrays');
                // Empty chunks are valid. Detached chunks are not.
                const bytes = frameBuffer(result.value);
                length += bytes.length;
                if (length > 24 * 1024 * 1024)
                    throw frameError('Image stream exceeds the 24 MiB input budget', 'NotSupportedError');
                chunks.push(new Uint8Array(bytes));
            }
            if (state.closed) return;
            state.bytes = new Uint8Array(length);
            let offset = 0;
            for (const chunk of chunks) { state.bytes.set(chunk, offset); offset += chunk.length; }
            state.complete = true;
            state.completed.resolve();
            state.reader = null;
            imageDecoderStreams.release(reader);
            startImageDecoder(state);
        } catch (error) { if (!state.closed) failImageDecoder(state, error); }
    };
    const decodedImageFrame = (state, result, pixels) => {
        let width = result.width, height = result.height;
        const options = state.options;
        if (options.desiredWidth && options.desiredHeight &&
            (options.desiredWidth !== width || options.desiredHeight !== height)) {
            const resized = formatImageBitmap({width, height, pixels}, {
                resizeWidth: options.desiredWidth, resizeHeight: options.desiredHeight,
                resizeQuality: 'high', imageOrientation: 'from-image', premultiplyAlpha: 'none'});
            width = resized.width; height = resized.height; pixels = resized.pixels;
        }
        const frame = frameStateOptions(width, height, 'RGBA', {
            timestamp: result.timestamp, duration: result.duration,
            rotation: result.rotation, flip: result.flip});
        frame.planes = [new Uint8Array(pixels)];
        return makeVideoFrame(frame);
    };
    class ImageDecoder {
        constructor(init) {
            if (arguments.length === 0) throw new TypeError('ImageDecoder requires an init dictionary');
            const options = imageDecoderOptions(init);
            const state = {options, type: options.type, complete: !options.stream,
                completed: decoderDeferred(), ready: decoderDeferred(),
                trackItems: [], selectedIndex: -1, closed: false,
                bytes: options.bytes, reader: null, id: null, timer: null,
                generation: 0, pending: new Set()};
            state.tracks = createImageTrackList(state);
            imageDecoderStates.set(this, state);
            if (options.stream) state.reader = imageDecoderStreams.reader(options.stream);
            else state.completed.resolve();
            for (const buffer of options.transfer) host('arrayBufferDetach', buffer);
            state.timer = setTimeout(() => {
                state.timer = null;
                if (state.closed) return;
                if (!host('imageDecoderSupported', options.mime)) {
                    failImageDecoder(state, frameError('Image media type is not supported', 'NotSupportedError'));
                    return;
                }
                if (state.reader) readImageDecoderStream(state);
                else startImageDecoder(state);
            }, 0);
        }
        get type() { return imageDecoderState(this).type; }
        get complete() { return imageDecoderState(this).complete; }
        get completed() { return imageDecoderState(this).completed.promise; }
        get tracks() { return imageDecoderState(this).tracks; }
        decode(options = {}) {
            try {
                const state = imageDecoderActive(this);
                const dictionary = frameDictionary(options);
                const completeFramesOnly = dictionary.completeFramesOnly;
                if (completeFramesOnly !== undefined) Boolean(completeFramesOnly);
                const rawIndex = dictionary.frameIndex;
                const frameIndex = rawIndex === undefined ? 0 : frameUint(rawIndex);
                if (state.pending.size >= 32)
                    throw frameError('Too many pending image-frame requests', 'NotSupportedError');
                return new Promise((resolve, reject) => {
                    const request = {resolve, reject, timer: null, generation: state.generation};
                    state.pending.add(request);
                    const finish = result => {
                        state.pending.delete(request);
                        resolve(result);
                    };
                    state.ready.promise.then(() => {
                        if (!state.pending.has(request)) return;
                        if (state.selectedIndex < 0) throw frameError('No image track is selected');
                        if (frameIndex >= imageTrackState(state.trackItems[state.selectedIndex]).frameCount)
                            throw new RangeError('Image frame index is out of range');
                        let pixels, result, offset = 0;
                        const copy = () => {
                            request.timer = null;
                            if (!state.pending.has(request)) return;
                            try {
                                result = host('imageDecoderFrame', state.id, frameIndex, offset, state.selectedIndex);
                                if (!pixels) {
                                    bitmapPixelBudget(result.width, result.height);
                                    pixels = new Uint8Array(result.width * result.height * 4);
                                }
                                pixels.set(result.pixels, offset);
                                offset += result.pixels.length;
                                if (result.done) {
                                    finish({image: decodedImageFrame(state, result, pixels), complete: true});
                                } else request.timer = setTimeout(copy, 0);
                            } catch (error) {
                                state.pending.delete(request);
                                reject(error);
                            }
                        };
                        request.timer = setTimeout(copy, 0);
                    }).catch(error => {
                        if (state.pending.delete(request)) reject(error);
                    });
                });
            } catch (error) { return Promise.reject(error); }
        }
        reset() { cancelImageDecodes(imageDecoderActive(this), frameError('Image decoder was reset', 'AbortError')); }
        close() { closeImageDecoder(imageDecoderState(this), frameError('Image decoder was closed', 'AbortError')); }
        static isTypeSupported(type) {
            try {
                if (arguments.length === 0) throw new TypeError('Image media type is required');
                const string = frameString(type);
                const mime = imageMime(string);
                return Promise.resolve(Boolean(host('imageDecoderSupported', mime)));
            } catch (error) { return Promise.reject(error); }
        }
    }
    Object.defineProperty(ImageDecoder.prototype, Symbol.toStringTag, {value: 'ImageDecoder', configurable: true});
    Object.defineProperty(ImageDecoder, 'isTypeSupported', {enumerable: true});
    frameIDL([ImageDecoder, ImageTrack, ImageTrackList]);
    if (host('imageDecoderSecureContext')) Object.assign(globalThis, {ImageDecoder, ImageTrack, ImageTrackList});
