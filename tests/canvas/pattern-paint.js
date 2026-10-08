function testPatternPainting(createCanvas) {
    const source=createCanvas();source.width=2;source.height=2;
    const s=source.getContext('2d');s.fillStyle='red';s.fillRect(0,0,1,2);
    s.fillStyle='blue';s.fillRect(1,0,1,2);
    const paints=[];
    for(const mode of ['native','clipped','scalar']) {
        const target=createCanvas();target.width=32;target.height=32;
        const c=target.getContext('2d'),p=c.createPattern(source,'repeat');
        c.imageSmoothingEnabled=false;c.fillStyle='green';c.fillRect(0,0,32,32);
        if(mode==='clipped'){c.beginPath();c.rect(0,0,32,32);c.clip();}
        // Each 32-pixel row is below native batching's 256-pixel admission.
        // Integer boundaries preserve coverage while keeping a scalar oracle.
        const fill=(top,height)=>{
            if(mode==='scalar')for(let y=top;y<top+height;y++)c.fillRect(0,y,32,1);
            else c.fillRect(0,top,32,height);
        };
        p.setTransform({a:2,d:2,e:1});c.fillStyle=p;c.globalAlpha=.5;
        fill(0,32);
        p.setTransform({e:1});c.globalAlpha=1;fill(16,16);
        paints.push(Array.from(c.getImageData(0,0,32,32).data));
    }
    for(let i=1;i<paints.length;i++)
        if(JSON.stringify(paints[0])!==JSON.stringify(paints[i]))throw Error('native, clipped and scalar pattern differ');
    const pixel=(x,y)=>paints[0].slice((y*32+x)*4,(y*32+x)*4+4);
    if(JSON.stringify(pixel(0,16))!=='[0,0,255,255]'||JSON.stringify(pixel(1,16))!=='[255,0,0,255]')
        throw Error('live transform repeat phase');
    return {samples:[pixel(0,0),pixel(1,0),pixel(0,16),pixel(1,16)],equal:true};
}
