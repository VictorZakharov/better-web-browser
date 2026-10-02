audioProbe('compressed reset aborts old flush without closing the decoder', async()=>{
    const entry = elementaryCases.find(item=>item.codec === 'mp3');
    const config = elementaryConfig(entry);
    requireAudio((await AudioDecoder.isConfigSupported(config)).supported, 'MP3 decoder unavailable');
    let count = 0, failure;
    const decoder = new AudioDecoder({output:audio=>{count++;audio.close();},error:error=>{failure=error;}});
    try {
        decoder.configure(config);
        decoder.decode(new EncodedAudioChunk({type:'key', timestamp:0,
            data:new Uint8Array(entry.packets[0])}));
        const pending = decoder.flush();
        decoder.reset();
        let aborted;
        try { await pending; } catch (error) { aborted = error; }
        requireAudio(aborted?.name === 'AbortError', 'reset rejects old flush');
        requireAudio(count === 0 && !failure, 'cancelled generation emits no callback');
        decoder.configure(config);
        decoder.decode(new EncodedAudioChunk({type:'key',timestamp:17,
            data:new Uint8Array(entry.packets[0])}));
        await decoder.flush();
        if (failure) throw failure;
        requireAudio(count > 0, 'fresh native generation decodes sound');
    } finally { if (decoder.state !== 'closed') decoder.close(); }
});

audioProbe('malformed packet closes only its codec', async()=>{
    const entry = elementaryCases.find(item=>item.codec === 'flac');
    requireAudio((await AudioDecoder.isConfigSupported(elementaryConfig(entry))).supported,
        'FLAC decoder unavailable');
    let outputs = 0, errors = 0, failure;
    const decoder = new AudioDecoder({output:audio=>{outputs++;audio.close();},
        error:error=>{errors++;failure=error;}});
    decoder.configure(elementaryConfig(entry));
    const corrupt = new Uint8Array(entry.packets[0]);
    corrupt[corrupt.length-1] ^= 1;
    decoder.decode(new EncodedAudioChunk({type:'key',timestamp:0,data:corrupt}));
    let rejected;
    try { await decoder.flush(); } catch (error) { rejected=error; }
    requireAudio(decoder.state === 'closed' && errors === 1 && outputs === 0,
        'one asynchronous failure and no placeholder output');
    requireAudio(failure?.name === 'EncodingError' && rejected?.name === 'EncodingError',
        'native error propagated to callback and flush');
    return decodeElementary(entry);
});
