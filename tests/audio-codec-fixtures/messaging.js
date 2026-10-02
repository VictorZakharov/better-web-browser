// The native Window/MessagePort serializer is independent of Worker cloning.
audioProbe('MessagePort audio resource aliasing and immutable samples', async () => {
    const channel = new MessageChannel();
    const audio = ownedAudio(16, 2, 48000, -45);
    const expected = new Float32Array(32);
    audio.copyTo(expected, {planeIndex:0});
    const received = new Promise(resolve => channel.port2.onmessage = resolve);
    channel.port1.postMessage({audio, alias:audio});
    audio.close();
    const value = (await received).data;
    requireAudio(value.audio instanceof AudioData && value.audio === value.alias,
        'receiver brand and alias');
    requireAudio(value.audio.timestamp === -45 && value.audio.numberOfChannels === 2,
        'receiver metadata');
    const actual = new Float32Array(32);
    value.audio.copyTo(actual, {planeIndex:0});
    sameSamples(actual, expected);
    value.audio.close();
    channel.port1.close();
    channel.port2.close();
});

audioProbe('MessagePort audio transfer commits atomically', async () => {
    const channel = new MessageChannel();
    const audio = ownedAudio(4);
    const buffer = new ArrayBuffer(4);
    audioFailure(() => channel.port1.postMessage({audio, invalid:()=>{}}, [audio, buffer]),
        'DataCloneError');
    requireAudio(audio.numberOfFrames === 4 && buffer.byteLength === 4,
        'failed graph leaves transfers live');
    const received = new Promise(resolve => channel.port2.onmessage = resolve);
    channel.port1.postMessage({audio, buffer}, [audio, buffer]);
    requireAudio(audio.numberOfFrames === 0 && buffer.byteLength === 0,
        'successful transfer detaches sender');
    const value = (await received).data;
    requireAudio(value.audio.numberOfFrames === 4 && value.buffer.byteLength === 4,
        'receiving resources remain live');
    value.audio.close();
    channel.port1.close();
    channel.port2.close();
});

audioProbe('MessagePort encoded chunks preserve packet bytes', async () => {
    const channel = new MessageChannel();
    const source = new Uint8Array([9,1,2,3,9]);
    const chunk = new EncodedAudioChunk({type:'key',timestamp:-10,duration:25,
        data:source.subarray(1,4)});
    const received = new Promise(resolve => channel.port2.onmessage = resolve);
    channel.port1.postMessage({chunk, alias:chunk});
    source.fill(0);
    const value = (await received).data;
    requireAudio(value.chunk instanceof EncodedAudioChunk && value.chunk === value.alias,
        'packet receiver identity');
    requireAudio(value.chunk.timestamp === -10 && value.chunk.duration === 25,
        'packet timing');
    const bytes = new Uint8Array(3);
    value.chunk.copyTo(bytes);
    sameSamples(bytes, [1,2,3]);
    audioFailure(() => channel.port1.postMessage(chunk, [chunk]), 'DataCloneError');
    channel.port1.close();
    channel.port2.close();
});
