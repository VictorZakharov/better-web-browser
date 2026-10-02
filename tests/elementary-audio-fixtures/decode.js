// cases.js is generated from the repository's original tone fixtures, not a
// network response. Each case contains elementary packets and separate headers.
const elementaryConfig = entry => ({codec:entry.codec, sampleRate:44100,
    numberOfChannels:1, ...(entry.description === null ? {} : {
        description:new Uint8Array(entry.description)})});

async function decodeElementary(entry, config = elementaryConfig(entry), snapshot = false) {
    const support = await AudioDecoder.isConfigSupported(config);
    requireAudio(support.supported, `decoder unavailable: ${entry.codec}`);
    let frames = 0, energy = 0, outputs = 0, failure;
    const timestamps = [];
    const decoder = new AudioDecoder({output:audio=>{
        try {
            requireAudio(audio.sampleRate === 44100, 'metadata sample rate');
            requireAudio(audio.numberOfChannels === 1, 'metadata channel count');
            requireAudio(audio.duration === Math.floor(audio.numberOfFrames * 1000000 / 44100),
                'duration uses actual sample rate');
            const samples = new Float32Array(audio.numberOfFrames);
            audio.copyTo(samples, {planeIndex:0, format:'f32-planar'});
            for (const sample of samples) {
                requireAudio(Number.isFinite(sample), 'nonfinite PCM');
                energy += sample * sample;
            }
            frames += audio.numberOfFrames;
            outputs++;
            timestamps.push(audio.timestamp);
        } catch (error) { failure ??= error; }
        finally { audio.close(); }
    }, error:error=>{failure ??= error;}});
    try {
        decoder.configure(support.config);
        if (snapshot && support.config.description) {
            const description = support.config.description;
            const bytes = ArrayBuffer.isView(description) ? new Uint8Array(description.buffer,
                description.byteOffset, description.byteLength) : new Uint8Array(description);
            bytes.fill(255);
        }
        for (let index = 0; index < entry.packets.length; index++) {
            const data = new Uint8Array(entry.packets[index]);
            decoder.decode(new EncodedAudioChunk({type:'key', timestamp:index * 26122, data}));
            if (snapshot) data.fill(255);
            if (decoder.decodeQueueSize >= 32) await decoder.flush();
        }
        await decoder.flush();
        if (failure) throw failure;
        requireAudio(decoder.decodeQueueSize === 0, 'queue fully drained');
        requireAudio(frames >= entry.minimumFrames, `decoded only ${frames} samples`);
        requireAudio(energy > 1, 'no actual tone');
        for (let index = 1; index < timestamps.length; index++) {
            requireAudio(timestamps[index] >= timestamps[index-1], 'monotonic chunk timestamps');
        }
        return `${frames} frames; ${outputs} outputs; energy ${energy.toFixed(3)}`;
    } finally { if (decoder.state !== 'closed') decoder.close(); }
}

for (const entry of elementaryCases) {
    audioProbe(`elementary ${entry.codec} ${entry.description === null ? 'in-band' : 'description'}`,
        () => decodeElementary(entry));
}

for (const entry of elementaryCases.filter(item=>item.description !== null)) {
    audioProbe(`snapshot ${entry.codec}`, async()=>{
        const backing = new Uint8Array(entry.description.length + 8);
        backing.set(entry.description, 4);
        const config = {...elementaryConfig(entry),
            description:new DataView(backing.buffer, 4, entry.description.length)};
        const pending = AudioDecoder.isConfigSupported(config);
        backing.fill(255);
        structuredClone(backing.buffer, {transfer:[backing.buffer]});
        const support = await pending;
        requireAudio(support.supported, 'detached source does not invalidate snapshot');
        return decodeElementary(entry, support.config, true);
    });
}

audioProbe('profile support query reports recognized configuration', async()=>{
    for (const codec of ['mp4a.40.5','mp4a.40.29','mp4a.40.42']) {
        const support = await AudioDecoder.isConfigSupported({codec, sampleRate:48000, numberOfChannels:1});
        requireAudio(typeof support.supported === 'boolean' && support.config.codec === codec,
            `invalid capability result: ${codec}`);
    }
});
