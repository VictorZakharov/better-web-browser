    // MediaCapabilities dictionary conversion and validity are distinct from
    // decoder support. Optional audio members remain optional.
    // https://w3c.github.io/media-capabilities/#valid-audio-configuration
    const mediaCapabilityString = value => `${value}`;
    const mediaCapabilityUnsigned = (value, bits) => {
        const number = +value;
        if (!Number.isFinite(number) || number === 0) return 0;
        const modulus = 2 ** bits;
        const integer = Math.trunc(number) % modulus;
        return integer < 0 ? integer + modulus : integer;
    };
    const mediaCapabilityDictionary = value => {
        if (value == null) return {};
        if (typeof value !== 'object' && typeof value !== 'function')
            throw new TypeError('Expected a media configuration dictionary');
        return value;
    };
    const mediaCapabilityRequired = (dictionary, name, convert) => {
        const value = dictionary[name];
        if (value === undefined) throw new TypeError('Missing media configuration ' + name);
        return convert(value);
    };
    const mediaCapabilityOptional = (dictionary, result, name, convert) => {
        const value = dictionary[name];
        if (value !== undefined) result[name] = convert(value);
    };
    const mediaCapabilityDefaulted = (dictionary, result, name, convert, fallback) => {
        const value = dictionary[name];
        result[name] = value === undefined ? fallback : convert(value);
    };
    const mediaCapabilityEnum = values => value => {
        const string = mediaCapabilityString(value);
        if (!values.includes(string)) throw new TypeError('Invalid media configuration enum');
        return string;
    };
    const mediaCapabilityDouble = value => {
        const number = +value;
        if (!Number.isFinite(number)) throw new TypeError('Expected a finite media configuration number');
        return number;
    };
    const mediaCapabilitySequence = value => {
        if (value === null || typeof value !== 'object' && typeof value !== 'function')
            throw new TypeError('Expected an iterable media configuration sequence');
        const method = value[Symbol.iterator];
        if (typeof method !== 'function') throw new TypeError('Expected an iterable media configuration sequence');
        const iterator = Reflect.apply(method, value, []);
        if (iterator === null || typeof iterator !== 'object' && typeof iterator !== 'function')
            throw new TypeError('Invalid media configuration iterator');
        const next = iterator.next;
        const result = [];
        // Web IDL converts each value as it is yielded, without Array.from's
        // array-like fallback or its different abrupt-conversion closing rules.
        for (;;) {
            const step = Reflect.apply(next, iterator, []);
            if (step === null || typeof step !== 'object' && typeof step !== 'function')
                throw new TypeError('Invalid media configuration iterator result');
            if (step.done) return result;
            result.push(mediaCapabilityString(step.value));
        }
    };
    const mediaCapabilityTrack = (value, media) => {
        const dictionary = mediaCapabilityDictionary(value);
        const result = {};
        if (media === 'audio') {
            mediaCapabilityOptional(dictionary, result, 'bitrate', value => mediaCapabilityUnsigned(value, 64));
            mediaCapabilityOptional(dictionary, result, 'channels', mediaCapabilityString);
            result.contentType = mediaCapabilityRequired(dictionary, 'contentType', mediaCapabilityString);
            mediaCapabilityOptional(dictionary, result, 'samplerate', value => mediaCapabilityUnsigned(value, 32));
            mediaCapabilityOptional(dictionary, result, 'spatialRendering', value => !!value);
        } else {
            result.bitrate = mediaCapabilityRequired(dictionary, 'bitrate', value => mediaCapabilityUnsigned(value, 64));
            mediaCapabilityOptional(dictionary, result, 'colorGamut', mediaCapabilityEnum(['srgb', 'p3', 'rec2020']));
            result.contentType = mediaCapabilityRequired(dictionary, 'contentType', mediaCapabilityString);
            result.framerate = mediaCapabilityRequired(dictionary, 'framerate', mediaCapabilityDouble);
            mediaCapabilityOptional(dictionary, result, 'hasAlphaChannel', value => !!value);
            mediaCapabilityOptional(dictionary, result, 'hdrMetadataType',
                mediaCapabilityEnum(['smpteSt2086', 'smpteSt2094-10', 'smpteSt2094-40']));
            result.height = mediaCapabilityRequired(dictionary, 'height', value => mediaCapabilityUnsigned(value, 32));
            mediaCapabilityOptional(dictionary, result, 'scalabilityMode', mediaCapabilityString);
            mediaCapabilityOptional(dictionary, result, 'spatialScalability', value => !!value);
            mediaCapabilityOptional(dictionary, result, 'transferFunction', mediaCapabilityEnum(['srgb', 'pq', 'hlg']));
            result.width = mediaCapabilityRequired(dictionary, 'width', value => mediaCapabilityUnsigned(value, 32));
        }
        return result;
    };
    const mediaCapabilityKeyTrack = value => {
        const dictionary = mediaCapabilityDictionary(value);
        const result = {};
        mediaCapabilityDefaulted(dictionary, result, 'encryptionScheme', value =>
            value === null ? null : mediaCapabilityString(value), null);
        mediaCapabilityDefaulted(dictionary, result, 'robustness', mediaCapabilityString, '');
        return result;
    };
    const mediaCapabilityKeySystem = value => {
        const dictionary = mediaCapabilityDictionary(value);
        const result = {};
        const requirement = mediaCapabilityEnum(['required', 'optional', 'not-allowed']);
        mediaCapabilityOptional(dictionary, result, 'audio', mediaCapabilityKeyTrack);
        mediaCapabilityDefaulted(dictionary, result, 'distinctiveIdentifier', requirement, 'optional');
        mediaCapabilityDefaulted(dictionary, result, 'initDataType', mediaCapabilityString, '');
        result.keySystem = mediaCapabilityRequired(dictionary, 'keySystem', mediaCapabilityString);
        mediaCapabilityDefaulted(dictionary, result, 'persistentState', requirement, 'optional');
        mediaCapabilityOptional(dictionary, result, 'sessionTypes', mediaCapabilitySequence);
        mediaCapabilityOptional(dictionary, result, 'video', mediaCapabilityKeyTrack);
        return result;
    };
    const mediaConfigurationSnapshot = value => {
        const dictionary = mediaCapabilityDictionary(value);
        const snapshot = {};
        // Inherited MediaConfiguration members precede the derived members;
        // conversion is recursive at each getter, not after reading all values.
        // https://webidl.spec.whatwg.org/#es-dictionary
        mediaCapabilityOptional(dictionary, snapshot, 'audio', value => mediaCapabilityTrack(value, 'audio'));
        mediaCapabilityOptional(dictionary, snapshot, 'video', value => mediaCapabilityTrack(value, 'video'));
        mediaCapabilityOptional(dictionary, snapshot, 'keySystemConfiguration', mediaCapabilityKeySystem);
        snapshot.type = mediaCapabilityRequired(dictionary, 'type',
            mediaCapabilityEnum(['file', 'media-source', 'webrtc']));
        const type = snapshot.type;
        if (!snapshot.audio && !snapshot.video)
            throw new TypeError('A media configuration requires audio or video');
        for (const media of ['audio', 'video']) {
            const track = snapshot[media];
            if (track && host('mediaCapabilitiesContentType', track.contentType, media, type) === 'invalid')
                throw new TypeError('Invalid ' + media + ' track MIME configuration');
        }
        if (snapshot.audio && type === 'webrtc' && 'spatialRendering' in snapshot.audio)
            throw new TypeError('spatialRendering is not applicable to WebRTC');
        if (snapshot.video) {
            if (snapshot.video.framerate <= 0) throw new TypeError('Video framerate must be positive');
            if (type !== 'webrtc' && 'scalabilityMode' in snapshot.video)
                throw new TypeError('scalabilityMode is applicable only to WebRTC decoding');
        }
        if (snapshot.keySystemConfiguration) {
            if (type === 'webrtc') throw new TypeError('WebRTC cannot use a keySystemConfiguration');
            const key = snapshot.keySystemConfiguration;
            if (key.audio && !snapshot.audio || key.video && !snapshot.video)
                throw new TypeError('Encrypted track configuration requires the corresponding track');
        }
        return snapshot;
    };
