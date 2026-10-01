'use strict';
registerFrameProbe('copy observes close during option conversion',async()=>{
    const frame=rgbaFrame();let name;
    try {await frame.copyTo(new Uint8Array(8),{get format(){frame.close();return 'RGBA';}});}
    catch(error){name=error.name;}
    frameAssert(name==='InvalidStateError','closed resource checked after dictionary conversion');
});
registerFrameProbe('copy options read once in IDL order',async()=>{
    const frame=rgbaFrame(),visits=[];
    try {
        const bytes=new Uint8Array(8);
        await frame.copyTo(bytes,{get colorSpace(){visits.push('color');return 'srgb';},
            get format(){visits.push('format');return 'RGBA';},
            get layout(){visits.push('layout');return undefined;},
            get rect(){visits.push('rect');return undefined;}});
        frameAssert(visits.join(',')==='color,format,layout,rect','single dictionary conversion: '+visits);
    } finally {frame.close();}
});
registerFrameProbe('frame-to-frame cropped display aspect', async () => {
    const source=new VideoFrame(new Uint8Array(32),{format:'RGBA',codedWidth:4,codedHeight:2,timestamp:0,displayWidth:12,displayHeight:8});
    const cropped=new VideoFrame(source,{visibleRect:{x:1,y:0,width:2,height:1}});
    try {frameAssert(cropped.displayWidth===6&&cropped.displayHeight===4,'scaled crop geometry');}
    finally {source.close();cropped.close();}
});
registerFrameProbe('complete padded source allocation', async () => {
    let name;
    try {new VideoFrame(new Uint8Array(12),{format:'RGBA',codedWidth:1,codedHeight:2,timestamp:0,layout:[{offset:0,stride:8}]});}
    catch(error){name=error.name;}
    frameAssert(name==='TypeError','last row padding is included in source allocation');
});
registerFrameProbe('WebIDL frame attributes are enumerable', async () => {
    const descriptor=Object.getOwnPropertyDescriptor(VideoFrame.prototype,'codedWidth');
    frameAssert(descriptor.enumerable&&descriptor.configurable,'IDL attribute descriptor');
});
for(const [format,chroma] of [['I422A',2],['I444A',4]]) {
    registerFrameProbe(format+' actual planar alpha',async()=>{
        const input=new Uint8Array(8+chroma*2);input.subarray(0,4).fill(235);
        input.subarray(4,4+chroma*2).fill(128);input.set([0,64,128,255],4+chroma*2);
        const frame=new VideoFrame(input,{format,codedWidth:2,codedHeight:2,timestamp:0});
        try {
            const output=new Uint8Array(frame.allocationSize());await frame.copyTo(output);equalBytes(output,[...input]);
            const context=new OffscreenCanvas(2,2).getContext('2d');context.drawImage(frame,0,0);
            const pixels=context.getImageData(0,0,2,2).data;
            equalBytes([pixels[3],pixels[7],pixels[11],pixels[15]],[0,64,128,255]);
        } finally {frame.close();}
    });
}
registerFrameProbe('RGBA bytes and timestamp', async () => {
    const frame = rgbaFrame();
    try {
        frameAssert(frame.timestamp===42&&frame.duration===30, 'microsecond metadata');
        const bytes = new Uint8Array(frame.allocationSize()); await frame.copyTo(bytes);
        equalBytes(bytes, [10,20,30,255,40,50,60,128]);
    } finally { frame.close(); }
});
registerFrameProbe('BGRA conversion', async () => {
    const frame = rgbaFrame();
    try {
        const bytes = new Uint8Array(frame.allocationSize({format:'BGRA'}));
        await frame.copyTo(bytes,{format:'BGRA'});
        equalBytes(bytes, [30,20,10,255,60,50,40,128]);
    } finally { frame.close(); }
});
registerFrameProbe('custom output stride', async () => {
    const frame = rgbaFrame();
    try {
        const options = {layout:[{offset:3,stride:12}]};
        frameAssert(frame.allocationSize(options)===15, 'complete last stride allocation');
        const bytes = new Uint8Array(15).fill(99); await frame.copyTo(bytes,options);
        equalBytes(bytes.subarray(3,11), [10,20,30,255,40,50,60,128]);
        frameAssert(bytes[0]===99&&bytes[14]===99, 'padding untouched');
    } finally { frame.close(); }
});
registerFrameProbe('input subview snapshot', async () => {
    const bytes = new Uint8Array([99,10,20,30,255,99]);
    const frame = new VideoFrame(bytes.subarray(1,5),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
    bytes.fill(0);
    try {
        const output = new Uint8Array(4); await frame.copyTo(output);
        equalBytes(output, [10,20,30,255]);
    } finally { frame.close(); }
});
registerFrameProbe('close and clone ownership', async () => {
    const frame = rgbaFrame(), clone = frame.clone();
    frame.close();
    try {
        frameAssert(frame.codedWidth===0&&frame.format===null&&frame.visibleRect===null, 'closed geometry');
        const output = new Uint8Array(8); await clone.copyTo(output);
        equalBytes(output, [10,20,30,255,40,50,60,128]);
    } finally { clone.close(); }
});
registerFrameProbe('structured clone graph identity', async () => {
    const frame = rgbaFrame();
    let cloned;
    try {
        cloned = structuredClone({a:frame,b:frame});
        frameAssert(cloned.a===cloned.b&&frame.codedWidth===2, 'clone reference graph');
        const output = new Uint8Array(8); await cloned.a.copyTo(output);
        equalBytes(output, [10,20,30,255,40,50,60,128]);
    } finally { frame.close(); cloned?.a.close(); }
});
registerFrameProbe('structured transfer detaches sender', async () => {
    const frame = rgbaFrame();
    const received = structuredClone(frame,{transfer:[frame]});
    try {
        frameAssert(frame.format===null&&received.format==='RGBA', 'frame transferred');
        const output = new Uint8Array(8); await received.copyTo(output);
        equalBytes(output, [10,20,30,255,40,50,60,128]);
    } finally { received.close(); }
});
registerFrameProbe('invalid transfer is atomic', async () => {
    const frame = rgbaFrame();
    try {
        let name;
        try { structuredClone(frame,{transfer:[frame,frame]}); } catch(error) { name=error.name; }
        frameAssert(name==='DataCloneError'&&frame.format==='RGBA', 'duplicate transfer rejected without detach');
    } finally { frame.close(); }
});
registerFrameProbe('VideoFrame Canvas source', async () => {
    const canvas = new OffscreenCanvas(1,1), context = canvas.getContext('2d');
    context.fillStyle='#ff0000';context.fillRect(0,0,1,1);
    const frame = new VideoFrame(canvas,{timestamp:0});
    context.clearRect(0,0,1,1);
    try {
        const bytes = new Uint8Array(frame.allocationSize({format:'RGBA'}));
        await frame.copyTo(bytes,{format:'RGBA'}); equalBytes(bytes,[255,0,0,255]);
    } finally { frame.close(); }
});
registerFrameProbe('VideoFrame drawImage', async () => {
    const frame = rgbaFrame();
    try {
        const canvas = new OffscreenCanvas(2,1), context = canvas.getContext('2d');
        context.drawImage(frame,0,0);
        const bytes = context.getImageData(0,0,2,1).data;
        equalBytes(bytes.subarray(0,4),[10,20,30,255]);
        frameAssert(bytes[7]===128, 'straight alpha enters Canvas once');
    } finally { frame.close(); }
});
registerFrameProbe('frame to ImageBitmap snapshot', async () => {
    const frame = rgbaFrame(); let bitmap;
    try {
        bitmap = await createImageBitmap(frame); frame.close();
        frameAssert(bitmap.width===2&&bitmap.height===1, 'bitmap resource survives source close');
    } finally { frame.close(); bitmap?.close(); }
});
for (const format of ['I420','I422','I444','NV12','I420A']) {
    registerFrameProbe(format+' plane roundtrip', async () => {
        const chromaCount = format==='I444'?8:format==='I422'?4:2;
        const bytes = new Uint8Array(4+chromaCount+(format==='I420A'?4:0));
        bytes.set([16,235,16,235]);bytes.fill(128,4,4+chromaCount);
        if (format==='I420A') bytes.set([1,2,3,4],bytes.length-4);
        const frame = new VideoFrame(bytes,{format,codedWidth:2,codedHeight:2,timestamp:0});
        try {
            const output = new Uint8Array(frame.allocationSize()); await frame.copyTo(output);
            equalBytes(output,Array.from(bytes));
        } finally { frame.close(); }
    });
}
registerFrameProbe('YUV limited-range Canvas pixels', async () => {
    const frame = new VideoFrame(new Uint8Array([16,235,16,235,128,128]),
        {format:'I420',codedWidth:2,codedHeight:2,timestamp:0});
    try {
        const canvas = new OffscreenCanvas(2,2), context = canvas.getContext('2d');
        context.drawImage(frame,0,0);
        equalBytes(context.getImageData(0,0,2,1).data,[0,0,0,255,255,255,255,255]);
    } finally { frame.close(); }
});
registerFrameProbe('closed frame rejects copy', async () => {
    const frame = rgbaFrame(); frame.close();
    let name; try { await frame.copyTo(new Uint8Array(8)); } catch(error) { name=error.name; }
    frameAssert(name==='InvalidStateError', 'closed copy rejection');
});
registerFrameProbe('short destination rejects without write', async () => {
    const frame = rgbaFrame();
    try {
        const bytes = new Uint8Array(7).fill(99); let name;
        try { await frame.copyTo(bytes); } catch(error) { name=error.name; }
        frameAssert(name==='TypeError'&&bytes.every(value=>value===99), 'copy validation before write');
    } finally { frame.close(); }
});
registerFrameProbe('RGB color-space metadata', async () => {
    const frame = rgbaFrame();
    try {
        const color = frame.colorSpace;
        frameAssert(color.primaries==='bt709'&&color.matrix==='rgb'&&color.fullRange===true,
            'actual RGB resource color space');
        const json = color.toJSON();json.matrix='bt709';
        frameAssert(color.matrix==='rgb', 'metadata snapshot');
    } finally { frame.close(); }
});
