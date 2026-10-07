function testClippedSolidPaint(make, clipCase) {
    const width=53,height=37;
    const draw=(scalar,opaque,operation,opacity,transformed)=>{
        const c=make(width,height).getContext('2d',{alpha:!opaque});
        const seed=c.createImageData(width,height);
        for(let index=0;index<width*height;index++)
            seed.data.set([index%256,71,193,(index*37)%256],index*4);
        c.putImageData(seed,0,0);
        c.beginPath();
        if(clipCase===0)c.rect(0,0,width,height);
        if(clipCase===1){c.rect(3.5,2.25,43,31);c.rect(17.5,9.25,15,11);}
        if(clipCase===2){c.moveTo(1,3);c.lineTo(49,6);c.lineTo(21,34);c.closePath();}
        if(clipCase===3)c.rect(0,0,0,0);
        c.clip('evenodd');
        if(clipCase===2){c.beginPath();c.rect(4,7,38,25);c.clip();}
        c.save();
        // A nested empty clip must not modify the saved region.
        c.beginPath();c.rect(0,0,0,0);c.clip();c.restore();
        if(transformed)c.setTransform(1,.125,-.25,1,7,1);
        const color='rgba(231,53,141,.75)';
        let paint=color;
        if(scalar){const g=c.createLinearGradient(0,0,53,0);g.addColorStop(0,color);g.addColorStop(1,color);paint=g;}
        c.fillStyle=paint;c.strokeStyle=paint;c.globalAlpha=opacity;
        c.lineWidth=2.75;c.lineCap='round';c.lineJoin='bevel';c.setLineDash([3,2]);c.lineDashOffset=.75;
        if(operation===0){c.beginPath();c.rect(1.25,3.75,44,28);c.arc(25,16,9,0,Math.PI*2);c.fill('evenodd');}
        if(operation===1){c.beginPath();c.moveTo(2,2);c.bezierCurveTo(45,2,3,35,49,30);c.stroke();}
        if(operation===2)c.fillRect(5,4,43,27);
        if(operation===3)c.strokeRect(5,4,43,27);
        if(operation===4){c.font='17px Arial';c.fillText('AV xy',1,24);}
        return c.getImageData(0,0,width,height).data;
    };
    let cases=0;
    for(const opaque of [false,true])for(let operation=0;operation<5;operation++)
        for(const opacity of [0,.25,1])for(const transformed of [false,true]){
            const native=draw(false,opaque,operation,opacity,transformed);
            const scalar=draw(true,opaque,operation,opacity,transformed);
            for(let index=0;index<native.length;index++)if(native[index]!==scalar[index])
                throw Error(`clip ${clipCase} case ${cases} byte ${index}: ${native[index]}/${scalar[index]}`);
            cases++;
        }
    return cases;
}
