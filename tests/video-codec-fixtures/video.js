async function decodedVideo(options = {}, count = 8) {
    const config={codec:'av01.0.04M.08',hardwareAcceleration:'prefer-software',...options};
    requireAudio((await VideoDecoder.isConfigSupported(config)).supported,'AV1 decoder unavailable');
    const frames=[];
    let failure;
    const decoder=new VideoDecoder({output:frame=>frames.push(frame),error:error=>{failure=error;}});
    try {
        decoder.configure(config);
        for(let index=0;index<count;index++)decoder.decode(new EncodedVideoChunk({
            type:index===0?'key':'delta',timestamp:index*250000,duration:250000,
            data:new Uint8Array(videoPackets[index])}));
        await decoder.flush();
        if(failure)throw failure;
        requireAudio(frames.length===count,'actual inter-frame output count');
        return frames;
    }catch(error){for(const frame of frames)frame.close();throw error;}
    finally {if(decoder.state!=='closed')decoder.close();}
}

audioProbe('AV1 persistent inter-frame decoding and independent pixel reference',async()=>{
    const frames=await decodedVideo();
    try {
        let maxDifference=0;
        for(let index=0;index<frames.length;index++) {
            const frame=frames[index];
            requireAudio(frame.codedWidth===16&&frame.codedHeight===16,'actual stream dimensions');
            requireAudio(frame.timestamp===index*250000&&frame.duration===250000,'chunk presentation metadata');
            const bytes=new Uint8Array(1024);
            await frame.copyTo(bytes,{format:'RGBA'});
            for(let pixel=0;pixel<bytes.length;pixel++)
                maxDifference=Math.max(maxDifference,Math.abs(bytes[pixel]-videoReference[index*1024+pixel]));
        }
        requireAudio(maxDifference<=3,`independent reference differs by ${maxDifference}`);
        return `8 actual frames; max pixel difference ${maxDifference}`;
    }finally{for(const frame of frames)frame.close();}
});

audioProbe('EncodedVideoChunk immutable view snapshot',()=>{
    const input=new Uint8Array([99,1,2,3,88]);
    const chunk=new EncodedVideoChunk({type:'key',timestamp:-7,data:new DataView(input.buffer,1,3)});
    input.fill(255);
    const output=new Uint8Array(3);chunk.copyTo(output);
    requireAudio(output.join(',')==='1,2,3'&&chunk.timestamp===-7&&chunk.duration===null,'private chunk snapshot');
});

audioProbe('EncodedVideoChunk native clone brand and bytes',()=>{
    const source=new EncodedVideoChunk({type:'key',timestamp:8,data:new Uint8Array(videoPackets[0])});
    const clone=structuredClone({a:source,b:source});
    requireAudio(clone.a instanceof EncodedVideoChunk&&clone.a===clone.b,'receiving brand and alias');
    const bytes=new Uint8Array(clone.a.byteLength);clone.a.copyTo(bytes);
    requireAudio(bytes.join(',')===videoPackets[0].join(','),'native chunk byte snapshot');
});

audioProbe('AV1 VideoFrame transfer and Canvas painting',async()=>{
    const frames=await decodedVideo({},1),source=frames[0];
    const moved=structuredClone(source,{transfer:[source]});
    try {
        requireAudio(source.format===null&&moved instanceof VideoFrame,'frame ownership moved');
        const canvas=new OffscreenCanvas(16,16),context=canvas.getContext('2d');
        context.drawImage(moved,0,0);
        const pixels=context.getImageData(0,0,16,16).data;
        requireAudio(pixels[0]>150&&pixels[3]===255,'actual decoded frame paints');
    }finally{moved.close();source.close();}
});

audioProbe('AV1 display aspect ratio and orientation',async()=>{
    const frames=await decodedVideo({displayAspectWidth:2,displayAspectHeight:1,rotation:80,flip:true},1);
    try {
        requireAudio(frames[0].displayWidth===16&&frames[0].displayHeight===32,'aspect expansion');
        requireAudio(frames[0].rotation===90&&frames[0].flip,'quarter-turn orientation');
    }finally{for(const frame of frames)frame.close();}
});

audioProbe('AV1 reset cancels old flush and stale outputs',async()=>{
    let outputs=0,failure;
    const decoder=new VideoDecoder({output:frame=>{outputs++;frame.close();},error:error=>{failure=error;}});
    try {
        decoder.configure({codec:'av01.0.04M.08'});
        decoder.decode(new EncodedVideoChunk({type:'key',timestamp:0,data:new Uint8Array(videoPackets[0])}));
        const pending=decoder.flush();decoder.reset();
        let rejected;try{await pending;}catch(error){rejected=error;}
        requireAudio(rejected?.name==='AbortError'&&!outputs&&!failure,'generation cancellation');
    }finally{if(decoder.state!=='closed')decoder.close();}
});

audioProbe('AV1 fresh configuration requires a key chunk',()=>{
    const decoder=new VideoDecoder({output:frame=>frame.close(),error:()=>{}});
    try {
        decoder.configure({codec:'av01.0.04M.08'});
        let rejected;
        try{decoder.decode(new EncodedVideoChunk({type:'delta',timestamp:0,data:new Uint8Array(videoPackets[1])}));}
        catch(error){rejected=error;}
        requireAudio(rejected?.name==='DataError','delta before key rejected');
    }finally{decoder.close();}
});
