    // Decode a complete encoded resource on a native worker, then transfer small
    // planar PCM chunks into an AudioBuffer on media tasks. Web Audio requires
    // synchronous detachment even though decoding itself runs asynchronously.
    const AudioArrayBuffer = globalThis.ArrayBuffer;
    const AudioUint8Array = globalThis.Uint8Array;
    const AudioFloat32Array = globalThis.Float32Array;
    const MAX_DECODE_INPUT_BYTES = 8 * 1024 * 1024;
    BaseAudioContext.prototype.decodeAudioData = function(
        audioData, successCallback = null, errorCallback = null) {
        if (!audioContextState.has(this))
            throw new TypeError('Illegal BaseAudioContext invocation');
        if (!(audioData instanceof AudioArrayBuffer))
            throw new TypeError('decodeAudioData requires an ArrayBuffer');
        if (successCallback !== null && typeof successCallback !== 'function')
            throw new TypeError('successCallback must be a function');
        if (errorCallback !== null && typeof errorCallback !== 'function')
            throw new TypeError('errorCallback must be a function');

        const rate = this.sampleRate;
        return new AudioPromise((resolve, reject) => {
            const fail = (error, inMediaTask = false) => {
                reject(error);
                if (errorCallback) {
                    if (inMediaTask) errorCallback(error);
                    else audioTask(() => errorCallback(error), 0);
                }
            };
            let bytes;
            let tooLarge;
            try {
                // Keep the encoded copy before V8 detaches the author's buffer.
                // The shared structuredClone bootstrap runs later than Web Audio.
                tooLarge = audioData.byteLength > MAX_DECODE_INPUT_BYTES;
                bytes = tooLarge ? null : new AudioUint8Array(audioData).slice();
                audioHost('arrayBufferDetach', audioData);
            } catch (error) {
                fail(new AudioDOMException('Audio data is detached', 'DataCloneError'));
                return;
            }
            if (tooLarge) {
                fail(new AudioDOMException('Encoded audio exceeds the 8 MiB decode limit',
                    'NotSupportedError'));
                return;
            }
            let id;
            try {
                id = audioHost('audioDecodeStart', bytes, rate);
            } catch (error) {
                fail(new AudioDOMException(String(error?.message || error), 'NotSupportedError'));
                return;
            }
            let buffer = null;
            const pump = () => {
                let result;
                try {
                    result = audioHost('audioDecodePoll', id);
                } catch (error) {
                    audioHost('audioDecodeCancel', id);
                    fail(error, true);
                    return;
                }
                if (result.status === 'pending') {
                    audioTask(pump, 10);
                    return;
                }
                if (result.status === 'error') {
                    fail(new AudioDOMException(result.message, 'EncodingError'), true);
                    return;
                }
                try {
                    if (result.status !== 'data')
                        throw new Error('Invalid audio decoder response');
                    if (!buffer) buffer = new AudioBuffer({
                        numberOfChannels: result.channels,
                        length: result.frames,
                        sampleRate: result.sampleRate
                    });
                    // The bridge returns f32 little-endian PCM; the Windows V8
                    // runtime and its Float32Array use that same representation.
                    const samples = new AudioFloat32Array(result.bytes.buffer,
                        result.bytes.byteOffset, result.bytes.byteLength / 4);
                    buffer.getChannelData(result.channel).set(samples, result.offset);
                } catch (error) {
                    audioHost('audioDecodeCancel', id);
                    fail(error, true);
                    return;
                }
                if (result.done) {
                    resolve(buffer);
                    if (successCallback) successCallback(buffer);
                } else audioTask(pump, 0);
            };
            audioTask(pump, 0);
        });
    };
