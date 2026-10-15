function testCanvasTextShadowRegions(make,operators) {
    const assert=(v,m)=>{if(!v)throw Error(m);};
    let cases=0;
    const render=(scalar,scenario,operator,opaque)=>{
        const canvas=make(192,128),c=canvas.getContext('2d',{alpha:!opaque});
        c.fillStyle='#36547991';c.fillRect(0,0,192,128);
        c.font='17px Arial';c.fillStyle='rgba(213,41,57,.7)';c.strokeStyle=c.fillStyle;
        if(scalar){
            const gradient=c.createLinearGradient(0,0,192,0);
            gradient.addColorStop(0,c.fillStyle);gradient.addColorStop(1,c.fillStyle);
            c.fillStyle=gradient;c.strokeStyle=gradient;
        }
        if(scenario===0){c.beginPath();c.rect(33,20,38,21);c.clip();}
        if(scenario===1){c.translate(9.25,2.5);c.rotate(.21);c.scale(.8,1.25);}
        if(scenario===2){c.textAlign='right';c.textBaseline='middle';}
        if(scenario===3){c.shadowOffsetX=-18.75;c.shadowOffsetY=-8.5;}
        if(scenario===4){c.shadowOffsetX=1e100;c.shadowOffsetY=-1e100;}
        if(scenario===5){c.filter='opacity(70%)';}
        if(scenario===6){
            const image=c.createImageData(1,1);image.data.set([223,101,37,0]);
            c.putImageData(image,180,120);
        }
        if(scenario===7||scenario===8){
            const paint=c.fillStyle;c.fillStyle='red';c.globalAlpha=.001;
            c.clearRect(125,90,67,38);
            if(scenario===7)c.fillRect(180,120,1,1);
            else c.fillText('tiny',130,110);
            c.fillStyle=paint;
        }
        c.globalAlpha=.63;c.globalCompositeOperation=operator;
        c.shadowColor='rgba(19,91,207,.7)';c.shadowBlur=scenario===3?0:8;
        if(scenario!==3&&scenario!==4){c.shadowOffsetX=1.5;c.shadowOffsetY=2.25;}
        c.lineWidth=1.5;
        c.fillText('Atlas AV',scenario===2?100:28.5,39.25,68.5);
        c.strokeText('xy',46.25,71.5);
        return c.getImageData(0,0,192,128).data;
    };
    for(const opaque of [false,true])for(const operator of operators) {
        for(let scenario=0;scenario<9;scenario++){
            const actual=render(false,scenario,operator,opaque),expected=render(true,scenario,operator,opaque);
            for(let i=0;i<actual.length;i++) {
                if(actual[i]!==expected[i]) throw Error(
                    'text shadow '+operator+' scenario '+scenario+' opaque '+opaque+' byte '+i+': '+actual[i]+'/'+expected[i]);
            }
            cases++;
        }
    }
    return cases;
}
function testCanvasTextShadowAtlas(make) {
    const assert=(v,m)=>{if(!v)throw Error(m);};
    const canvas=make(2048,1024),c=canvas.getContext('2d');
    c.font='18px Arial';c.shadowColor='black';c.shadowBlur=8;c.shadowOffsetY=1;
    for(let i=0;i<24;i++){
        c.clearRect(0,0,220,64);c.fillText('Damage '+i,12,32);
        assert(c.getImageData(0,0,220,64).data.some(n=>n),'repeated label missing');
    }
    assert(!c.getImageData(900,500,1,1).data.some(n=>n),'label affected unrelated pixels');
    const bitmap=c.getImageData(0,0,220,64).data;
    canvas.width=2048;
    c.font='18px Arial';c.shadowColor='black';c.shadowBlur=8;c.shadowOffsetY=1;
    c.fillText('Damage 23',12,32);
    const reset=c.getImageData(0,0,220,64).data;
    for(let i=0;i<reset.length;i++)assert(reset[i]===bitmap[i],'reset shadow mismatch');
    return 2;
}
