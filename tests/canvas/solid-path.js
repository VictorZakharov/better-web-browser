function testSolidPathPainting() {
    const native = new OffscreenCanvas(40,40).getContext('2d');
    const scalar = new OffscreenCanvas(40,40).getContext('2d');
    let cases = 0;
    const shapes = [
        c=>c.rect(2.25,3.75,25.5,24.25),
        c=>{c.moveTo(2,3);c.lineTo(30,5);c.lineTo(8,29);c.closePath();},
        c=>{c.arc(18,18,12,0,Math.PI*2);c.rect(12,12,12,12);},
        c=>{c.moveTo(2,19);c.bezierCurveTo(5,0,28,38,35,5);},
        c=>{c.roundRect(3,3,28,28,[1,4,{x:8,y:3},2]);}
    ];
    for (const shape of shapes) for (const transform of [[1,0,0,1,0,0],
        [1,.1,.2,1,0,0],[-1,0,0,1,39,0]]) for (const color of
        ['rgba(239,31,137,.5)','#00000000','#ffffff','rgba(7,97,197,.125)']) {
        for (const fill of [true,false]) {
            for (const c of [native,scalar]) {
                c.reset();
                const seed=c.createImageData(40,40);
                for(let index=0;index<seed.data.length;index+=4)
                    seed.data.set([31,71,113,(index/4)%256],index);
                c.putImageData(seed,0,0);
                // The constant gradient plus clip selects the scalar fallback;
                // clipping alone also uses native solid painting now.
                if(c===scalar){c.beginPath();c.rect(0,0,40,40);c.clip();}
                c.setTransform(...transform);c.beginPath();shape(c);
                let paint=color;
                if(c===scalar){const g=c.createLinearGradient(0,0,40,0);g.addColorStop(0,color);g.addColorStop(1,color);paint=g;}
                c.globalAlpha=.75;c.fillStyle=paint;c.strokeStyle=paint;
                c.lineWidth=2.25;c.lineCap='round';c.lineJoin='bevel';c.setLineDash([3,2]);
                c.lineDashOffset=.5;
                if(fill)c.fill('evenodd');else c.stroke();
            }
            const first=native.getImageData(0,0,40,40).data;
            const second=scalar.getImageData(0,0,40,40).data;
            for(let index=0;index<first.length;index++)if(first[index]!==second[index])
                throw Error(`fused path byte mismatch ${cases}/${index}: ${first[index]} != ${second[index]}`);
            cases++;
        }
    }
    return {cases};
}
