    const audioReadSample = (view, offset, kind) => {
        if (kind === 'u8') return (view.getUint8(offset) - 128) / 128;
        if (kind === 's16') return view.getInt16(offset, true) / 32768;
        if (kind === 's32') return view.getInt32(offset, true) / 2147483648;
        return view.getFloat32(offset, true);
    };
    const audioWriteSample = (view, offset, kind, sample, integerSource) => {
        if (kind === 'f32') { view.setFloat32(offset, sample, true); return; }
        // Integer conversion saturates rather than wrapping. Integer PCM's
        // asymmetric interval is normalized by the negative endpoint's scale.
        const finite = Number.isNaN(sample) ? 0 : sample;
        const clamped = Math.max(-1, Math.min(1, finite));
        // Integer narrowing discards low bits (arithmetic shift), rather than
        // rounding the midpoint into a different representable sample.
        const quantize = integerSource ? Math.floor : Math.round;
        if (kind === 'u8') view.setUint8(offset, Math.min(255, quantize(clamped * 128 + 128)));
        else if (kind === 's16') view.setInt16(offset, Math.min(32767, quantize(clamped * 32768)), true);
        else view.setInt32(offset, Math.min(2147483647, quantize(clamped * 2147483648)), true);
    };
    const copyAudioSamples = (state, destination, layout) => {
        if (destination.byteLength < layout.size) throw new RangeError('Audio destination is too small');
        const source = new DataView(state.bytes.buffer, state.bytes.byteOffset, state.bytes.byteLength);
        const target = new DataView(destination.buffer, destination.byteOffset, destination.byteLength);
        const original = audioFormatInfo(state.format);
        const channels = layout.info.planar ? 1 : state.channels;
        for (let frame = 0; frame < layout.count; frame++) {
            for (let channel = 0; channel < channels; channel++) {
                const sourceChannel = layout.info.planar ? layout.plane : channel;
                const sourceFrame = frame + layout.offset;
                const sourceIndex = original.planar ? sourceChannel * state.frames + sourceFrame :
                    sourceFrame * state.channels + sourceChannel;
                const targetIndex = frame * channels + channel;
                if (original.kind === layout.info.kind) {
                    // Same-type copies preserve float NaN payloads and signed
                    // zero, even when rearranging planar/interleaved storage.
                    const start = sourceIndex * original.size;
                    destination.set(state.bytes.subarray(start, start + original.size), targetIndex * original.size);
                } else {
                    audioWriteSample(target, targetIndex * layout.info.size, layout.info.kind,
                        audioReadSample(source, sourceIndex * original.size, original.kind), original.kind !== 'f32');
                }
            }
        }
    };
    const audioInterleavedFloat = state => {
        const bytes = new Uint8Array(state.frames * state.channels * 4);
        copyAudioSamples(state, bytes, audioCopyLayout(state,
            {format: 'f32', offset: 0, plane: 0, count: state.frames}));
        return bytes;
    };
