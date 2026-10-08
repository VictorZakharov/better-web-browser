'use strict';
(() => {
    const context=canvas.getContext('2d',{willReadFrequently:true});
    const source=new OffscreenCanvas(3,2),sourceContext=source.getContext('2d');
    sourceContext.putImageData(new ImageData(new Uint8ClampedArray([
        255,0,0,255,0,255,0,128,0,0,255,255,
        255,255,0,255,0,255,255,128,255,0,255,255
    ]),3,2),0,0);
    const kinds=['linear','radial','conic','pattern'];
    const times={},checksums={},measurements={};
    let stage=0,batch=0,expected=null;
    function paint(kind) {
        let style;
        if(kind==='linear')style=context.createLinearGradient(.5,0,95.5,0);
        else if(kind==='radial')style=context.createRadialGradient(64,64,1,64,64,70);
        else if(kind==='conic')style=context.createConicGradient(.25,64,64);
        else {
            style=context.createPattern(source,'repeat');
            style.setTransform({a:2,b:.125,c:-.25,d:2,e:3.5,f:-2.75});
        }
        if(kind!=='pattern') {
            style.addColorStop(0,'rgba(197,43,239,.5)');
            style.addColorStop(.5,'rgba(11,71,173,0)');
            style.addColorStop(.5,'rgba(87,63,19,.7)');
            style.addColorStop(1,'rgb(219,181,113)');
        }
        context.fillStyle=style;context.strokeStyle=style;
        context.save();context.beginPath();context.rect(7,9,109,103);context.clip();
        context.lineWidth=1.75;context.lineCap='round';context.lineJoin='bevel';
        context.setLineDash([3.5,1.25]);context.lineDashOffset=-.5;
        for(let index=0;index<12;index++) {
            context.globalAlpha=.37+(index%3)*.125;
            context.setTransform(1,.1,-.2,1,7.25,3.5);
            context.beginPath();context.moveTo(1.25,1.5);
            context.bezierCurveTo(20,120,95,-10,117.75,111.25);
            context.lineTo(15.5,97.25);context.closePath();
            if(index%2)context.stroke();else context.fill('evenodd');
        }
        context.restore();
    }
    function run() {
        const kind=kinds[stage];
        if(batch===0)measurements[kind]=[];
        canvas.width=128;
        const start=performance.now();paint(kind);
        const pixels=context.getImageData(0,0,128,128).data;
        const elapsed=performance.now()-start;
        if(batch)measurements[kind].push(elapsed);
        let checksum=2166136261;
        for(const byte of pixels)checksum=Math.imul(checksum^byte,16777619)>>>0;
        if(expected!==null && expected!==checksum)throw Error('unstable '+kind+' path pixels');
        expected=checksum;
        if(++batch<4){setTimeout(run,0);return;}
        const sorted=measurements[kind].slice().sort((a,b)=>a-b);
        times[kind]=sorted[1];checksums[kind]=expected;
        probe.setAttribute('data-ms',JSON.stringify(times));
        probe.setAttribute('data-checksums',JSON.stringify(checksums));
        probe.setAttribute('data-measurements',JSON.stringify(measurements));
        probe.textContent=JSON.stringify({times,checksums,measurements});
        batch=0;expected=null;
        if(++stage<kinds.length)setTimeout(run,0);
        else probe.setAttribute('data-done','true');
    }
    setTimeout(run,0);
})();
