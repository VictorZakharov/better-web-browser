// The caller supplies a real CC0-derived SFNT fixture with a GSUB fi ligature.
// Both Window and Worker execute this against their own native font registry.
async function testFontFeatureSettings(bytes, set) {
    const assert=(value,message)=>{if(!value)throw Error(message);};
    const face=new FontFace('OwnedFeatureFixture',bytes,{featureSettings:"'liga' off"});
    assert(face.featureSettings==='"liga" 0','descriptor CSS serialization');
    await face.load();
    set.add(face);
    const canvas=new OffscreenCanvas(100,80),ctx=canvas.getContext('2d');
    ctx.font='20px OwnedFeatureFixture';
    assert(ctx.measureText('fi').width===40,'descriptor disables real GSUB ligature');
    ctx.fillText('fi',10,40);
    let pixels=ctx.getImageData(0,0,100,80).data,count=0;
    for(let i=3;i<pixels.length;i+=4)if(pixels[i])count++;
    assert(count===800,'two actual Ahem glyph outlines');
    face.featureSettings="'liga' on";
    assert(face.status==='loaded','descriptor update does not reload bytes');
    assert(ctx.measureText('fi').width===20,'loaded descriptor invalidates native run cache');
    ctx.clearRect(0,0,100,80);ctx.fillText('fi',10,40);
    pixels=ctx.getImageData(0,0,100,80).data;count=0;
    for(let i=3;i<pixels.length;i+=4)if(pixels[i])count++;
    assert(count===400,'one actual replacement glyph outline');
    let invalid=false;
    try{face.featureSettings="'bad'";}catch(e){invalid=e.name==='SyntaxError';}
    assert(invalid&&face.featureSettings==='"liga"','invalid mutation preserves descriptor');
    assert(ctx.measureText('fi').width===20,'invalid mutation preserves installed font');
    face.featureSettings='normal';
    assert(ctx.measureText('fi').width===20,'normal returns to font default features');
    set.delete(face);
    return 'passed';
}
