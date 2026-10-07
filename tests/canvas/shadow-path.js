function testShadowPathPainting(mode) {
    const contexts = [new OffscreenCanvas(32,24).getContext('2d'),
        new OffscreenCanvas(32,24).getContext('2d')];
    let cases = 0;
    for (const fill of [true,false]) for (const external of [true,false])
        for (const transform of [[1,0,0,1,0,0],[1,.1,.2,1,0,0],[-1,0,0,1,31,0]]) {
        for (const [index,c] of contexts.entries()) {
            c.reset();
            const seed = c.createImageData(32,24);
            for (let byte=0;byte<seed.data.length;byte+=4)
                seed.data.set([31,71,113,(byte/4)%256],byte);
            c.putImageData(seed,0,0);
            c.beginPath();c.rect(4,2,23,20);c.clip();
            c.setTransform(...transform);
            const path = external ? new Path2D() : c;
            if (!external) c.beginPath();
            path.moveTo(2.25,2);path.lineTo(25,3.5);path.lineTo(10,20);path.closePath();
            c.fillStyle=c.strokeStyle='rgba(197,43,239,.5)';c.globalAlpha=.75;
            c.lineWidth=2.25;c.lineJoin='bevel';c.lineCap='round';c.setLineDash([3,2]);
            c.lineDashOffset=.5;c.shadowColor='rgba(10,70,220,.625)';
            c.shadowBlur=2.5;c.shadowOffsetX=.5;c.shadowOffsetY=-.25;
            c.globalCompositeOperation=mode;
            // Identity opacity preserves source bytes but selects the existing
            // filter/source-layer pipeline rather than fused shadow geometry.
            if (index) c.filter='opacity(1)';
            if (fill) external ? c.fill(path,'evenodd') : c.fill('evenodd');
            else external ? c.stroke(path) : c.stroke();
        }
        const actual=contexts[0].getImageData(0,0,32,24).data;
        const expected=contexts[1].getImageData(0,0,32,24).data;
        for (let byte=0;byte<actual.length;byte++) if (actual[byte]!==expected[byte])
            throw Error(`shadow path mismatch ${mode}/${cases}/${byte}: ${actual[byte]} != ${expected[byte]}`);
        cases++;
    }
    return cases;
}
