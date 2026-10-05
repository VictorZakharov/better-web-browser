function testPatternPainting(createCanvas) {
    const source=createCanvas();source.width=2;source.height=2;
    const s=source.getContext('2d');s.fillStyle='red';s.fillRect(0,0,1,2);
    s.fillStyle='blue';s.fillRect(1,0,1,2);
    const paints=[];
    for(const clipped of [false,true]) {
        const target=createCanvas();target.width=32;target.height=32;
        const c=target.getContext('2d'),p=c.createPattern(source,'repeat');
        c.imageSmoothingEnabled=false;c.fillStyle='green';c.fillRect(0,0,32,32);
        if(clipped){c.beginPath();c.rect(0,0,32,32);c.clip();}
        p.setTransform({a:2,d:2,e:1});c.fillStyle=p;c.globalAlpha=.5;
        c.fillRect(0,0,32,32);
        p.setTransform({e:1});c.globalAlpha=1;c.fillRect(0,16,32,16);
        paints.push(Array.from(c.getImageData(0,0,32,32).data));
    }
    if(JSON.stringify(paints[0])!==JSON.stringify(paints[1]))throw Error('native and clipped managed pattern differ');
    const pixel=(x,y)=>paints[0].slice((y*32+x)*4,(y*32+x)*4+4);
    if(JSON.stringify(pixel(0,16))!=='[0,0,255,255]'||JSON.stringify(pixel(1,16))!=='[255,0,0,255]')
        throw Error('live transform repeat phase');
    return {samples:[pixel(0,0),pixel(1,0),pixel(0,16),pixel(1,16)],equal:true};
}
