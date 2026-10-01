    const imageDecoderStates = new WeakMap();
    const imageTrackStates = new WeakMap();
    const imageTrackListStates = new WeakMap();
    const imageTrackToken = Symbol('ImageTrack');
    const imageTrackListToken = Symbol('ImageTrackList');
    let imageDecoderStreams = globalThis.__imageDecoderStreamBindings;
    delete globalThis.__imageDecoderStreamBindings;
    if (!imageDecoderStreams) globalThis.__bindImageDecoderStreams = bindings => { imageDecoderStreams = bindings; };
    const decoderDeferred = () => {
        let resolve, reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        // The spec marks completed and tracks.ready handled. Applications may
        // elect not to observe these independently from a rejected decode().
        promise.catch(() => {});
        return {promise, resolve, reject};
    };
    const imageMime = type => {
        const value = frameString(type);
        const match = /^(image)\/([!#$%&'*+.^_`|~0-9a-z-]+)(?:[ \t]*;[ \t]*[!#$%&'*+.^_`|~0-9a-z-]+=(?:[!#$%&'*+.^_`|~0-9a-z-]+|"(?:[\t\x20\x21\x23-\x5b\x5d-\x7e\x80-\xff]|\\[\t\x20-\x7e\x80-\xff])*"))*$/i.exec(value);
        if (!match) throw new TypeError('ImageDecoder requires a valid image MIME type');
        return 'image/' + match[2].toLowerCase();
    };
    const imageDecoderOptions = value => {
        const options = frameDictionary(value);
        const colorSpaceConversion = frameEnum(options.colorSpaceConversion, ['none', 'default'], 'default');
        const data = frameRequired(options.data, 'data');
        const rawHeight = options.desiredHeight;
        const desiredHeight = rawHeight === undefined ? undefined : frameUint(rawHeight);
        const rawWidth = options.desiredWidth;
        const desiredWidth = rawWidth === undefined ? undefined : frameUint(rawWidth);
        const animation = options.preferAnimation;
        const preferAnimation = animation === undefined ? undefined : Boolean(animation);
        const transfer = frameTransfers(options.transfer);
        const type = frameString(frameRequired(options.type, 'type'));
        const mime = imageMime(type);
        if ((desiredHeight === undefined) !== (desiredWidth === undefined))
            throw new TypeError('Desired image dimensions must be supplied together');
        const stream = imageDecoderStreams?.has(data) === true;
        let bytes;
        if (stream) {
            if (imageDecoderStreams.unusable(data)) throw new TypeError('ImageDecoder stream is locked or disturbed');
            // Reader acquisition occurs only after all dictionary conversions.
        } else {
            bytes = frameBuffer(data);
            if (!bytes.length) throw new TypeError('Encoded image must not be empty');
            if (bytes.length > 24 * 1024 * 1024)
                throw frameError('Encoded image exceeds the 24 MiB budget', 'NotSupportedError');
            bytes = new Uint8Array(bytes);
        }
        frameValidateTransfers(transfer);
        return {type, mime, bytes, stream: stream ? data : null, transfer,
            preferAnimation, desiredHeight, desiredWidth, colorSpaceConversion};
    };
    const imageDecoderState = value => {
        const state = imageDecoderStates.get(value);
        if (!state) throw new TypeError('Illegal ImageDecoder receiver');
        return state;
    };
    const imageDecoderActive = value => {
        const state = imageDecoderState(value);
        if (state.closed) throw frameError('ImageDecoder is closed');
        return state;
    };
