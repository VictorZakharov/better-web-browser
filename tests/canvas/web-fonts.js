async function testCanvasWebFonts(bytes) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const canvas=document.createElement('canvas');canvas.width=140;canvas.height=80;
    const context=canvas.getContext('2d');
    context.font='20px CanvasOwnedAhem';
    const fallback=context.measureText('ABC').width;
    const face=new FontFace('CanvasOwnedAhem',bytes);
    await face.load();
    assert(face.status==='loaded','actual font bytes decoded');
    assert(Math.abs(context.measureText('ABC').width-fallback)<0.001,
        'loaded but unregistered face does not affect Canvas');
    document.fonts.add(face);
    assert(Math.abs(context.measureText('ABC').width-60)<0.001,'registered face shapes exact advances');
    context.fillStyle='#ff0000';context.fillText('A',10,40);
    const data=context.getImageData(0,0,140,80).data;
    let painted=0;
    for(let y=0;y<80;y++)for(let x=0;x<140;x++) {
        const index=(y*140+x)*4;
        const inside=x>=10&&x<30&&y>=24&&y<44;
        if(data[index+3])painted++;
        assert(inside ? data[index]===255&&data[index+1]===0&&data[index+2]===0&&data[index+3]===255 :
            data[index+3]===0,'registered face rasterizes the actual Ahem outline');
    }
    assert(painted===400,'font paints a real twenty pixel square');
    const offscreen=new OffscreenCanvas(140,80).getContext('2d');
    offscreen.font='20px CanvasOwnedAhem';
    assert(Math.abs(offscreen.measureText('ABC').width-60)<0.001,'Window OffscreenCanvas uses document fonts');
    document.fonts.delete(face);
    assert(Math.abs(context.measureText('ABC').width-fallback)<0.001,'removal invalidates font selection');
    const replacement=new FontFace('CanvasOwnedReplacement',bytes);
    await replacement.load();document.fonts.add(replacement);
    context.font='20px CanvasOwnedReplacement';
    assert(Math.abs(context.measureText('ABC').width-60)<0.001,'same-count replacement is not skipped');
    document.fonts.clear();
    assert(Math.abs(context.measureText('ABC').width-60)>0.1,'clear removes loaded faces from Canvas');
    return 'passed';
}
