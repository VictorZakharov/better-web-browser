async function encodeFlac(frames, channels, rate, level, blockSize) {
    const config = {codec:'flac', sampleRate:rate, numberOfChannels:channels,
        flac:{compressLevel:level, blockSize}};
    requireAudio((await AudioEncoder.isConfigSupported(config)).supported, 'FLAC encoder unavailable');
    const packets = [];
    let metadata, failure;
    const encoder = new AudioEncoder({output:(chunk, info)=>{
        packets.push(chunk);
        metadata ??= info.decoderConfig;
        try {
            requireAudio(chunk.type === 'key', 'independent FLAC frame');
            const bytes = new Uint8Array(chunk.byteLength);
            chunk.copyTo(bytes);
            requireAudio(bytes[0] === 255 && (bytes[1] & 254) === 248, 'frame-only chunk');
        } catch (error) { failure ??= error; }
    }, error:error=>{failure ??= error;}});
    try {
        encoder.configure(config);
        const audio = ownedAudio(frames, channels, rate, -900);
        encoder.encode(audio);
        audio.close();
        await encoder.flush();
        if (failure) throw failure;
        requireAudio(metadata?.codec === 'flac', 'registered metadata codec');
        const description = new Uint8Array(metadata.description);
        requireAudio(description.length === 42, 'separate STREAMINFO');
        requireAudio(String.fromCharCode(...description.slice(0,4)) === 'fLaC', 'description marker');
        requireAudio(packets.length === Math.ceil(frames / blockSize), 'actual frame packetization');
        return {packets, metadata};
    } finally { if (encoder.state !== 'closed') encoder.close(); }
}

async function verifyFlacRoundtrip(frames, channels, rate, level, blockSize) {
    const {packets, metadata} = await encodeFlac(frames, channels, rate, level, blockSize);
    let actualFrames = 0, energy = 0, failure;
    const decoder = new AudioDecoder({output:audio=>{
        try {
            requireAudio(audio.sampleRate === rate && audio.numberOfChannels === channels,
                'FLAC metadata dimensions');
            const samples = new Float32Array(audio.numberOfFrames * channels);
            audio.copyTo(samples, {planeIndex:0, format:'f32'});
            for (const sample of samples) energy += sample * sample;
            actualFrames += audio.numberOfFrames;
        } catch (error) { failure ??= error; }
        finally { audio.close(); }
    }, error:error=>{failure ??= error;}});
    try {
        decoder.configure({...metadata, sampleRate:1, numberOfChannels:32});
        for (const packet of packets) decoder.decode(packet);
        await decoder.flush();
        if (failure) throw failure;
        requireAudio(actualFrames === frames, `tail padding: ${actualFrames} versus ${frames}`);
        requireAudio(energy > .1, 'actual lossless sound');
        return `${actualFrames} frames; ${packets.length} raw packets; level ${level}`;
    } finally { if (decoder.state !== 'closed') decoder.close(); }
}

for (const level of [0,5,8]) {
    audioProbe(`FLAC encode effort ${level}`, ()=>verifyFlacRoundtrip(1025, 2, 48000, level, 512));
}
for (const rate of [8000,44100,96000]) {
    audioProbe(`FLAC encode rate ${rate}`, ()=>verifyFlacRoundtrip(777, 1, rate, 5, 256));
}
