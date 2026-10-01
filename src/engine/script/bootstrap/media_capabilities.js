    const mediaCapabilityTrackSupported = (track, media, mode) => {
        if (!track) return true;
        if (host('mediaCapabilitiesContentType', track.contentType, media, mode) !== 'supported')
            return false;
        if (media === 'audio') {
            return host('mediaCapabilitiesAudioSupported', track.contentType,
                track.channels, track.samplerate, track.spatialRendering === true, mode);
        }
        // The contained video renderer has finite pixel/frame budgets. It does
        // not implement HDR, alpha presentation or spatial/scalable output.
        return track.width > 0 && track.height > 0 && track.width <= 8192 && track.height <= 8192 &&
            track.width * track.height * 4 <= 128 * 1024 * 1024 &&
            !track.hasAlphaChannel && !track.hdrMetadataType && !track.spatialScalability &&
            (!track.colorGamut || track.colorGamut === 'srgb') &&
            (!track.transferFunction || track.transferFunction === 'srgb');
    };
    const mediaCapabilitiesWorker = !('document' in windowObject);
    class MediaCapabilities {
        constructor() { throw new TypeError('Illegal constructor'); }
        encodingInfo(configuration) {
            let snapshot;
            try {
                if (this !== ownedMediaCapabilities) throw new TypeError('Invalid MediaCapabilities receiver');
                snapshot = mediaConfigurationSnapshot(configuration, true);
            } catch (error) { return Promise.reject(error); }
            const audio = snapshot.audio;
            const supported = snapshot.type === 'record' && !snapshot.video && !!audio &&
                host('mediaCapabilitiesContentType', audio.contentType, 'audio', 'record') === 'supported' &&
                host('mediaCapabilitiesEncodingAudioSupported', audio.contentType,
                    audio.channels, audio.samplerate, audio.bitrate);
            return new Promise(resolve => queueMediaTask(() => resolve({
                supported, smooth: false, powerEfficient: false, configuration: snapshot
            })));
        }
        decodingInfo(configuration) {
            let snapshot;
            try {
                if (this !== ownedMediaCapabilities) throw new TypeError('Invalid MediaCapabilities receiver');
                snapshot = mediaConfigurationSnapshot(configuration);
            }
            catch (error) { return Promise.reject(error); }
            if (snapshot.keySystemConfiguration && mediaCapabilitiesWorker)
                return Promise.reject(new DOMException('Encrypted decoding queries require a Window', 'InvalidStateError'));
            if (snapshot.keySystemConfiguration && !host('mediaCapabilitiesSecureContext'))
                return Promise.reject(new DOMException('Encrypted decoding queries require a secure context', 'SecurityError'));
            const supported = snapshot.type !== 'webrtc' && !snapshot.keySystemConfiguration &&
                mediaCapabilityTrackSupported(snapshot.audio, 'audio', snapshot.type) &&
                mediaCapabilityTrackSupported(snapshot.video, 'video', snapshot.type);
            // Capability alone is not performance or power-efficiency evidence.
            // Queue ordinary media work rather than settling in the calling task.
            return new Promise(resolve => queueMediaTask(() => resolve({
                supported, smooth: false, powerEfficient: false,
                keySystemAccess: null, configuration: snapshot
            })));
        }
    }
    for (const method of ['encodingInfo', 'decodingInfo'])
        Object.defineProperty(MediaCapabilities.prototype, method, {enumerable: true});
    Object.defineProperty(MediaCapabilities.prototype, Symbol.toStringTag, {
        value: 'MediaCapabilities', configurable: true
    });
    Object.defineProperty(windowObject, 'MediaCapabilities', {
        value: MediaCapabilities, writable: true, configurable: true
    });
    const ownedMediaCapabilities = Object.create(MediaCapabilities.prototype);
    Object.defineProperty(windowObject.navigator, 'mediaCapabilities', {
        enumerable: true, configurable: true, get: () => ownedMediaCapabilities
    });
