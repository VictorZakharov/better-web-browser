'use strict';
(() => {
    const context=canvas.getContext('2d',{willReadFrequently:true});
    const times={},checksums={},measurements={};
    let stage=0,batch=0,expected=null;
    const cases=['fractionalFill','affineFill','affineClear'];
    function draw(kind,index) {
        context.fillStyle=index%2?'rgba(17,99,211,.7)':'rgba(219,83,29,.4)';
        context.globalAlpha=.73;
        if(kind==='fractionalFill') context.fillRect(.25,.5,126.5,126.25);
        else if(kind==='affineFill') {
            context.setTransform(1,.125,.25,1,1.5,-4.25);
            context.fillRect(0,0,100.5,100.25);
        } else {
            context.resetTransform();context.globalAlpha=1;
            context.fillStyle='#969696';context.fillRect(0,0,128,128);
            context.save();context.beginPath();context.rect(10,0,80,128);context.clip();
            context.setTransform(1,.125,.25,1,1.5,-4.25);
            context.clearRect(0,0,100.5,100.25);context.restore();
        }
    }
    function run() {
        const kind=cases[stage];
        if(batch===0)measurements[kind]=[];
        canvas.width=128;
        const start=performance.now();
        for(let index=0;index<8;index++) draw(kind,index);
        const pixels=context.getImageData(0,0,128,128).data;
        const elapsed=performance.now()-start;
        if(batch) measurements[kind].push(elapsed);
        let checksum=2166136261;
        for(const byte of pixels)checksum=Math.imul(checksum^byte,16777619)>>>0;
        if(expected!==null && checksum!==expected)throw Error('unstable '+kind+' pixels');
        expected=checksum;
        if(++batch<4){setTimeout(run,0);return;}
        const sorted=measurements[kind].slice().sort((a,b)=>a-b);
        times[kind]=sorted[1];checksums[kind]=expected;
        probe.setAttribute('data-ms',JSON.stringify(times));
        probe.setAttribute('data-checksums',JSON.stringify(checksums));
        probe.setAttribute('data-measurements',JSON.stringify(measurements));
        probe.textContent=JSON.stringify({times,checksums,measurements});
        batch=0;expected=null;
        if(++stage<cases.length)setTimeout(run,0);
        else probe.setAttribute('data-done','true');
    }
    setTimeout(run,0);
})();
