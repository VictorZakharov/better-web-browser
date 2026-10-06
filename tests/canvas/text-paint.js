function testCanvasTextPaint(make) {
    const assert=(v,m)=>{if(!v)throw Error(m);};
    const operators=['source-over','source-in','source-out','source-atop','destination-over',
        'destination-in','destination-out','destination-atop','copy','xor','lighter','multiply',
        'screen','overlay','darken','lighten','color-dodge','color-burn','hard-light','soft-light',
        'difference','exclusion','hue','saturation','color','luminosity'];
    const render=(operator,scenario,scalar,opaque)=>{
        const surface=make(96,64),c=surface.getContext('2d',{alpha:!opaque});
        c.fillStyle='#384a6099';c.fillRect(0,0,96,64);
        c.font='18px Arial';c.globalCompositeOperation=operator;
        c.globalAlpha=0.65;
        let paint='rgba(200,30,40,0.7)';
        if(scalar) {
            const gradient=c.createLinearGradient(0,0,96,0);
            gradient.addColorStop(0,paint);gradient.addColorStop(1,paint);paint=gradient;
        }
        c.fillStyle=paint;c.strokeStyle=paint;c.lineWidth=1.25;
        if(scenario===1) {
            c.beginPath();c.rect(12,8,65,40);c.clip();
            c.translate(4.25,2.75);c.rotate(0.15);
        }
        if(scenario===2) {
            c.shadowColor='rgba(25,90,180,0.7)';c.shadowBlur=2;
            c.shadowOffsetX=2;c.shadowOffsetY=3;
        }
        if(scenario===3) { c.textAlign='right';c.textBaseline='middle';c.scale(0.75,1.1); }
        c.fillText('AV A',scenario===3?85:6.5,31.25,51);
        c.strokeText('xy',9.75,50.5,28);
        return c.getImageData(0,0,96,64).data;
    };
    let cases=0;
    for(const opaque of [false,true])for(const operator of operators)for(let scenario=0;scenario<4;scenario++) {
        const native=render(operator,scenario,false,opaque),scalar=render(operator,scenario,true,opaque);
        assert(native.length===scalar.length,'text bitmap dimensions');
        for(let i=0;i<native.length;i++)if(native[i]!==scalar[i])assert(false,
            'native/scalar text byte '+i+' '+operator+' scenario='+scenario+' opaque='+opaque+
            ': '+native[i]+'/'+scalar[i]);
        cases++;
    }
    return cases;
}
