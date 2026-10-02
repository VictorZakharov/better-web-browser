audioProbe('support query snapshots recognized dictionaries', async () => {
    const config = {codec:'opus',sampleRate:48000,numberOfChannels:1,unknown:7,
        opus:{format:'opus',complexity:3,unknown:9}};
    const result = await AudioEncoder.isConfigSupported(config);
    requireAudio(result.supported,'real Opus encoder');
    requireAudio(!('unknown' in result.config) && !('unknown' in result.config.opus),'unknown members');
    config.opus.complexity = 10;
    requireAudio(result.config.opus.complexity === 3,'nested snapshot');
});

audioProbe('actual Opus samples and callback ordering', async () => {
    const packets = [],frames = [],order = [];
    let config;
    const encoder = new AudioEncoder({output:(packet,metadata)=>{
        packets.push(packet);config ??= metadata.decoderConfig;order.push('output');
    },error:error=>{throw error;}});
    encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:2,bitrate:96000});
    const input = ownedAudio(4800,2);
    encoder.encode(input);input.close();
    requireAudio(encoder.encodeQueueSize === 1 && packets.length === 0,'asynchronous acceptance');
    await encoder.flush();order.push('flush');
    requireAudio(packets.length >= 5 && order.at(-1) === 'flush','flush after output');
    const decoder = new AudioDecoder({output:frame=>frames.push(frame),error:error=>{throw error;}});
    decoder.configure(config);
    for (const packet of packets) decoder.decode(packet);
    await decoder.flush();
    let energy = 0, count = 0;
    for (const frame of frames) {
        const samples = new Float32Array(frame.numberOfFrames*2);
        frame.copyTo(samples,{planeIndex:0,format:'f32'});
        for (const sample of samples) {
            requireAudio(Number.isFinite(sample),'finite native output');energy += sample*sample;
        }
        count += frame.numberOfFrames;frame.close();
    }
    requireAudio(energy > 1 && count >= 4800,'nonzero actual decoded audio');
    encoder.close();decoder.close();
    return `packets=${packets.length}; decodedFrames=${count}; energy=${energy.toFixed(3)}`;
});

audioProbe('OpusHead description applies initial pre-skip', async () => {
    const packets = [],frames = [];let config;
    const encoder = new AudioEncoder({output:(packet,metadata)=>{
        packets.push(packet);config ??= metadata.decoderConfig;
    },error:error=>{throw error;}});
    encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{format:'ogg'}});
    encoder.encode(ownedAudio());await encoder.flush();
    requireAudio(config.description instanceof ArrayBuffer,'OpusHead description');
    const header = new Uint8Array(config.description);
    requireAudio(String.fromCharCode(...header.slice(0,8)) === 'OpusHead','identification header');
    const decoder = new AudioDecoder({output:frame=>frames.push(frame),error:error=>{throw error;}});
    decoder.configure(config);
    for(const packet of packets)decoder.decode(packet);
    await decoder.flush();
    requireAudio(frames.length > 0 && frames[0].timestamp === 0,'priming timestamp');
    const detail = `preSkip=${header[10]+header[11]*256}; firstFrames=${frames[0].numberOfFrames}`;
    for(const frame of frames)frame.close();encoder.close();decoder.close();return detail;
});

audioProbe('reset aborts accepted flush without output', async () => {
    let outputs = 0,errors = 0;
    const encoder = new AudioEncoder({output:()=>outputs++,error:()=>errors++});
    encoder.configure({codec:'opus',sampleRate:48000,numberOfChannels:1});
    encoder.encode(ownedAudio());const promise = encoder.flush();encoder.reset();
    let failure;try{await promise;}catch(error){failure=error;}
    requireAudio(failure?.name === 'AbortError' && encoder.state === 'unconfigured','abort state');
    requireAudio(outputs === 0 && errors === 0 && encoder.encodeQueueSize === 0,'cancelled queue');
    encoder.close();
});

audioProbe('registered signed PCM decoder is lossless', async () => {
    const config = {codec:'pcm-s24',sampleRate:48000,numberOfChannels:2};
    const support = await AudioDecoder.isConfigSupported(config);
    requireAudio(support.supported,'registered 24-bit decoder');
    const frames = [];
    const decoder = new AudioDecoder({output:frame=>frames.push(frame),error:error=>{throw error;}});
    decoder.configure(config);
    decoder.decode(new EncodedAudioChunk({type:'key',timestamp:17,
        data:new Uint8Array([0,0,128,255,255,127,255,255,255,1,0,0])}));
    await decoder.flush();
    requireAudio(frames.length === 1 && frames[0].timestamp === 17,'packet output metadata');
    const samples = new Int32Array(4);
    frames[0].copyTo(samples,{planeIndex:0,format:'s32'});
    sameSamples(samples,[-2147483648,2147483392,-256,256]);
    frames[0].close();decoder.close();
});
