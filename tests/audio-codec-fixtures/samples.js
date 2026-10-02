audioProbe('immutable sample view', () => {
    const storage = new Float32Array([99, -.5, .25, 99]);
    const audio = new AudioData({format:'f32',sampleRate:48000,numberOfChannels:1,
        numberOfFrames:2,timestamp:-17,data:storage.subarray(1,3)});
    storage.fill(0);
    const actual = new Float32Array(2);
    audio.copyTo(actual,{planeIndex:0});
    sameSamples(actual,[-.5,.25]);
    requireAudio(audio.timestamp === -17 && audio.duration === 41,'microsecond metadata');
    audio.close();
});

audioProbe('planar and interleaved channel extraction', () => {
    const audio = new AudioData({format:'s16-planar',sampleRate:8000,numberOfChannels:2,
        numberOfFrames:3,timestamp:0,data:new Int16Array([-32768,0,16384,32767,-16384,0])});
    const interleaved = new Int16Array(6);
    audio.copyTo(interleaved,{planeIndex:0,format:'s16'});
    sameSamples(interleaved,[-32768,32767,0,-16384,16384,0]);
    const right = new Float32Array(2);
    audio.copyTo(right,{planeIndex:1,format:'f32-planar',frameOffset:1,frameCount:2});
    sameSamples(right,[-.5,0]);
    audio.close();
});

audioProbe('integer narrowing discards low bits', () => {
    const audio = new AudioData({format:'s32',sampleRate:48000,numberOfChannels:1,
        numberOfFrames:5,timestamp:0,data:new Int32Array([1073741823,-1,-1073741825,2147483647,-2147483648])});
    const signed = new Int16Array(5),unsigned = new Uint8Array(5);
    audio.copyTo(signed,{planeIndex:0,format:'s16'});
    audio.copyTo(unsigned,{planeIndex:0,format:'u8'});
    // Implementations may convert through float32; the common browser contract
    // permits one destination LSB. Breeze's separate unit test checks its exact
    // arithmetic narrowing, including the low source bits Chrome discards.
    sameSamples(signed,[16383,-1,-16385,32767,-32768],1);
    sameSamples(unsigned,[191,127,63,255,0],1);audio.close();
});

audioProbe('destination views retain sentinels', () => {
    const audio = new AudioData({format:'f32',sampleRate:48000,numberOfChannels:2,
        numberOfFrames:3,timestamp:0,data:new Float32Array([1,2,3,4,5,6])});
    const target = new Float32Array(8).fill(123);
    audio.copyTo(target.subarray(2,6),{planeIndex:0,frameOffset:1,frameCount:2});
    sameSamples(target,[123,123,3,4,5,6,123,123]);
    requireAudio(audio.allocationSize({planeIndex:0,frameCount:2}) === 16,'allocation includes channels');
    audio.close();
});

audioProbe('copy bounds and plane validation', () => {
    const audio = ownedAudio(5,2);
    audioFailure(()=>audio.copyTo(new Float32Array(1),{planeIndex:0}),'RangeError');
    audioFailure(()=>audio.allocationSize({planeIndex:1}),'RangeError');
    audioFailure(()=>audio.allocationSize({planeIndex:2,format:'f32-planar'}),'RangeError');
    audioFailure(()=>audio.allocationSize({planeIndex:0,frameOffset:5}),'RangeError');
    audioFailure(()=>audio.allocationSize({planeIndex:0,frameCount:6}),'RangeError');
    requireAudio(audio.allocationSize({planeIndex:0,frameCount:0}) === 0,'empty valid prefix');
    audio.close();
});

audioProbe('independent clone and closed metadata', () => {
    const audio = ownedAudio(7,1,48000,100);
    const clone = audio.clone();
    audio.close();audio.close();
    requireAudio(audio.format === null && audio.sampleRate === 0 && audio.numberOfFrames === 0
        && audio.numberOfChannels === 0 && audio.timestamp === 100,'closed metadata');
    requireAudio(clone.numberOfFrames === 7,'clone retains immutable resource');
    audioFailure(()=>audio.clone(),'InvalidStateError');
    audioFailure(()=>audio.copyTo(new Uint8Array(28),{planeIndex:0}),'InvalidStateError');
    clone.close();
});

audioProbe('AudioData structured transfer commits atomically', () => {
    const audio = ownedAudio(7);
    audioFailure(()=>structuredClone({audio,uncloneable:()=>{}},{transfer:[audio]}),'DataCloneError');
    requireAudio(audio.numberOfFrames === 7,'failed serialization preserves source');
    const copy = structuredClone({first:audio,second:audio},{transfer:[audio]});
    requireAudio(copy.first === copy.second && copy.first.numberOfFrames === 7,'aliasing');
    requireAudio(audio.numberOfFrames === 0,'successful transfer closes source');
    copy.first.close();
});

audioProbe('encoded chunks snapshot BufferSource offsets', () => {
    const data = new Uint8Array([99,1,2,3,99]);
    const chunk = new EncodedAudioChunk({type:'key',timestamp:-9,duration:7,data:data.subarray(1,4)});
    data.fill(0);
    const target = new Uint8Array(5).fill(99);
    chunk.copyTo(target.subarray(1,4));
    sameSamples(target,[99,1,2,3,99]);
    requireAudio(chunk.byteLength === 3 && chunk.timestamp === -9 && chunk.duration === 7,'chunk metadata');
    audioFailure(()=>chunk.copyTo(new Uint8Array(2)),'TypeError');
    const clone = structuredClone(chunk);
    const clonedBytes = new Uint8Array(3);clone.copyTo(clonedBytes);
    sameSamples(clonedBytes,[1,2,3]);
});

audioProbe('constructor transfers after complete validation', () => {
    const storage = new Uint8Array([1,2,3,4]);
    audioFailure(()=>new AudioData({format:'s16',sampleRate:0,numberOfChannels:1,
        numberOfFrames:2,timestamp:0,data:storage,transfer:[storage.buffer]}),'TypeError');
    requireAudio(storage.byteLength === 4,'invalid dictionary must not detach');
    const audio = new AudioData({format:'s16',sampleRate:8000,numberOfChannels:1,
        numberOfFrames:2,timestamp:0,data:storage,transfer:[storage.buffer]});
    requireAudio(storage.byteLength === 0,'successful constructor detaches');
    const actual = new Uint8Array(4);audio.copyTo(actual,{planeIndex:0});
    sameSamples(actual,[1,2,3,4]);audio.close();
});
