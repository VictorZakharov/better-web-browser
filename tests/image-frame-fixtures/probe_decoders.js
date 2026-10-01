'use strict';
registerFrameProbe('invalid image MIME rejects support query', async () => {
    let name;try{await ImageDecoder.isTypeSupported('text/plain');}catch(error){name=error.name;}
    frameAssert(name==='TypeError','invalid MIME support query rejects, not success');
});
registerFrameProbe('valid unsupported image MIME resolves false', async () => {
    frameAssert(!(await ImageDecoder.isTypeSupported('image/x-owned-unknown-format')),'unsupported codec must not claim support');
});
registerFrameProbe('JPEG orientation retained separately from coded pixels',async()=>{
    const decoder=await decodeFixture('oriented.jpg','image/jpeg');
    try {
        const {image}=await decoder.decode();
        try {
            frameAssert(image.codedWidth===2&&image.codedHeight===1&&image.rotation===90,'coded JPEG geometry and orientation');
            frameAssert(image.displayWidth===1&&image.displayHeight===2,'oriented display');
            const raw=new Uint8Array(image.allocationSize({format:'RGBA'}));await image.copyTo(raw,{format:'RGBA'});
            const context=new OffscreenCanvas(1,2).getContext('2d');context.drawImage(image,0,0);
            equalBytes(context.getImageData(0,0,1,2).data,[...raw]);
        } finally {image.close();}
    } finally {decoder.close();}
});
registerFrameProbe('PNG ImageDecoder actual RGBA output', async () => {
    const decoder = await decodeFixture('rgba.png','image/png');
    try {
        const result = await decoder.decode();
        try {
            frameAssert(result.complete&&result.image.timestamp===0, 'still output metadata');
            const bytes = new Uint8Array(result.image.allocationSize({format:'RGBA'}));
            await result.image.copyTo(bytes,{format:'RGBA'});
            equalBytes(bytes,[10,20,30,255,40,50,60,128]);
        } finally { result.image.close(); }
    } finally { decoder.close(); }
});
registerFrameProbe('GIF previous-frame disposal', async () => {
    const decoder = await decodeFixture('disposal.gif','image/gif');
    try {
        frameAssert(decoder.tracks.selectedTrack.frameCount===3, 'GIF frame count');
        const {image} = await decoder.decode({frameIndex:2});
        try {
            const bytes = new Uint8Array(image.allocationSize({format:'RGBA'}));
            await image.copyTo(bytes,{format:'RGBA'});
            equalBytes(bytes.subarray(4,8),[255,0,0,255]);
            equalBytes(bytes.subarray(8,12),[0,0,255,255]);
            frameAssert(image.timestamp===90000&&image.duration===60000, 'GIF timing');
        } finally { image.close(); }
    } finally { decoder.close(); }
});
registerFrameProbe('APNG frame composition', async () => {
    const decoder = await decodeFixture('composition.png','image/png');
    try {
        const {image} = await decoder.decode({frameIndex:2});
        try {
            const bytes = new Uint8Array(image.allocationSize({format:'RGBA'}));
            await image.copyTo(bytes,{format:'RGBA'});
            equalBytes(bytes.subarray(4,8),[255,0,0,255]);
            equalBytes(bytes.subarray(8,12),[0,0,255,255]);
            frameAssert(image.timestamp===80000&&image.duration===40000, 'APNG timing');
        } finally { image.close(); }
    } finally { decoder.close(); }
});
registerFrameProbe('APNG distinct still track', async () => {
    const decoder = await decodeFixture('poster.png','image/png',{preferAnimation:false});
    try {
        frameAssert(decoder.tracks.length===2&&!decoder.tracks.selectedTrack.animated,
            'still selection: tracks='+decoder.tracks.length+', selected animated='+decoder.tracks.selectedTrack?.animated);
        const {image} = await decoder.decode();
        try {
            const bytes = new Uint8Array(image.allocationSize({format:'RGBA'}));
            await image.copyTo(bytes,{format:'RGBA'});
            equalBytes(bytes,[90,80,70,255,90,80,70,255,90,80,70,255,90,80,70,255]);
        } finally { image.close(); }
        const animated = [decoder.tracks[0],decoder.tracks[1]].find(track=>track.animated);
        animated.selected=true;
        const result=await decoder.decode({frameIndex:2});result.image.close();
    } finally { decoder.close(); }
});
registerFrameProbe('WebP animation random access', async () => {
    const decoder = await decodeFixture('animation.webp','image/webp');
    try {
        frameAssert(decoder.tracks.selectedTrack.frameCount===3, 'WebP frame count');
        const {image} = await decoder.decode({frameIndex:2});
        try {
            const bytes = new Uint8Array(image.allocationSize({format:'RGBA'}));
            await image.copyTo(bytes,{format:'RGBA'});
            equalBytes(bytes,[0,0,255,255,0,0,255,255,0,0,255,255,0,0,255,255]);
            frameAssert(image.timestamp===80000&&image.duration===40000, 'WebP timing');
        } finally { image.close(); }
    } finally { decoder.close(); }
});
registerFrameProbe('ImageDecoder reset aborts pending decode', async () => {
    const decoder = new ImageDecoder({type:'image/png',data:fixtureBytes('rgba.png')});
    try {
        const pending = decoder.decode();decoder.reset();
        let name;try{await pending;}catch(error){name=error.name;}
        frameAssert(name==='AbortError', 'reset promise rejection');
        const result = await decoder.decode();result.image.close();
    } finally { decoder.close(); }
});
registerFrameProbe('ImageDecoder close removes tracks', async () => {
    const decoder = await decodeFixture('rgba.png','image/png');
    const list = decoder.tracks;decoder.close();
    frameAssert(list.length===0&&list.selectedIndex===-1&&list.selectedTrack===null,'live list after close');
});
registerFrameProbe('decoded frame survives decoder close', async () => {
    const decoder = await decodeFixture('rgba.png','image/png');
    const {image} = await decoder.decode();decoder.close();
    try {
        const output = new Uint8Array(image.allocationSize({format:'RGBA'}));
        await image.copyTo(output,{format:'RGBA'});equalBytes(output,[10,20,30,255,40,50,60,128]);
    } finally { image.close(); }
});
registerFrameProbe('ImageDecoder chunked stream input', async () => {
    const data = fixtureBytes('rgba.png');
    const stream = new ReadableStream({start(controller){
        controller.enqueue(data.subarray(0,8));controller.enqueue(data.subarray(8));controller.close();
    }});
    const decoder = new ImageDecoder({type:'image/png',data:stream});
    try {
        await decoder.completed;
        const {image} = await decoder.decode();
        frameAssert(image.codedWidth===2&&decoder.complete,'stream owns actual decoded bytes');image.close();
    } finally { decoder.close(); }
});
registerFrameProbe('frame input transfer detaches buffer', async () => {
    const pixels = new Uint8Array([1,2,3,255]);
    const frame = new VideoFrame(pixels,{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0,
        transfer:[pixels.buffer]});
    try {
        frameAssert(pixels.byteLength===0,'input detached');
        const output = new Uint8Array(4);await frame.copyTo(output);equalBytes(output,[1,2,3,255]);
    } finally { frame.close(); }
});
