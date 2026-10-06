function testCanvasCompositeLayers(createCanvas) {
    const modes = ['source-over','source-in','source-out','source-atop','destination-over',
        'destination-in','destination-out','destination-atop','xor','copy','lighter',
        'multiply','screen','overlay','darken','lighten','color-dodge','color-burn',
        'hard-light','soft-light','difference','exclusion','hue','saturation','color','luminosity'];
    const samples = [];
    const sample = (c,x,y) => Array.from(c.getImageData(x,y,1,1).data);
    const draw = (size, mode, clipped, shadow, source, backdrop) => {
        const c = createCanvas(size,size).getContext('2d');
        c.fillStyle = backdrop; c.fillRect(0,0,size,size);
        if (clipped) { c.beginPath();c.rect(size/2,0,size/2,size);c.clip(); }
        c.globalCompositeOperation = mode;c.fillStyle = source;
        if (shadow) { c.shadowColor='rgba(20,80,160,.5)';c.shadowOffsetX=size/2; }
        c.fillRect(0,size/4,size/2,size/2);
        return [sample(c,size/4,size/2),sample(c,size*3/4,size/2),sample(c,size*3/4,0)];
    };
    for (const mode of modes) for (const clipped of [false,true]) for (const shadow of [false,true]) {
        for (const [source,backdrop] of [
            ['rgba(200,40,80,.5)','rgba(20,100,220,.5)'],['transparent','#123456'],
            ['white','black'],['black','white']
        ]) {
            const large=draw(32,mode,clipped,shadow,source,backdrop);
            const small=draw(8,mode,clipped,shadow,source,backdrop);
            if (JSON.stringify(large)!==JSON.stringify(small))
                throw Error(`composite size disagreement ${mode}/${clipped}/${shadow}: ${JSON.stringify(large)} vs ${JSON.stringify(small)}`);
            if (!clipped&&!shadow&&source==='white'&&backdrop==='black'&&mode==='color-dodge'&&large[0].join()!=='0,0,0,255')
                throw Error('color-dodge black backdrop must precede white source');
            if (!clipped&&!shadow&&source==='black'&&backdrop==='white'&&mode==='color-burn'&&large[0].join()!=='255,255,255,255')
                throw Error('color-burn white backdrop must precede black source');
            samples.push([mode,clipped,shadow,source,backdrop,large]);
        }
    }
    // The source layer is generated before the drawing clip, including shadows.
    const c=createCanvas(32,32).getContext('2d');
    c.rect(16,0,16,32);c.clip();c.shadowColor='blue';c.shadowOffsetX=16;
    c.fillStyle='red';c.fillRect(0,8,8,8);
    if (sample(c,2,10).join()!=='0,0,0,0'||sample(c,18,10).join()!=='0,0,255,255')
        throw Error('source/shadow clip order');
    return {cases:samples.length,samples};
}
