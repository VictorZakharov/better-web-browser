async function testFontUnicodeRanges(bytes) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const set=typeof document==='undefined'?fonts:document.fonts;
    const canvas=new OffscreenCanvas(100,60),context=canvas.getContext('2d');
    context.font='20px RangeOwnedAhem';
    const fallback=context.measureText('A').width;
    context.fillText('A',10,40);
    const fallbackPixels=context.getImageData(0,0,100,60).data;
    const upper=new FontFace('RangeOwnedAhem',bytes,{unicodeRange:'u+0041-005a'});
    const lower=new FontFace('RangeOwnedAhem',bytes,{unicodeRange:'U+61-7A'});
    assert(upper.unicodeRange==='U+41-5A','canonical descriptor from CSS grammar');
    set.add(upper);set.add(lower);
    assert(!set.check('20px RangeOwnedAhem','A'),'matching unloaded uppercase face');
    assert(set.check('20px RangeOwnedAhem','0'),'excluded text needs no face');
    assert(set.check('20px RangeOwnedAhem',''),'empty text needs no face');
    const excluded=await set.load('20px RangeOwnedAhem','0');
    assert(Array.isArray(excluded)&&excluded.length===0,'load excludes unused subsets');
    await Promise.all([upper.loaded,lower.loaded]);
    const loaded=await set.load('20px RangeOwnedAhem','Aa');
    assert(loaded.length===2&&loaded[0]===upper&&loaded[1]===lower,'actual composite subsets');
    assert(context.measureText('Aa').width===40,'both actual Ahem subsets shape');
    context.clearRect(0,0,100,60);context.fillText('A',10,40);
    let ink=0;for(const [index,byte] of context.getImageData(0,0,100,60).data.entries())
        if(index%4===3&&byte)ink++;
    assert(ink===400,'included character paints real Ahem glyph');
    upper.unicodeRange='U+42';
    assert(context.measureText('A').width===fallback,'range setter invalidates native selection/run caches');
    context.clearRect(0,0,100,60);context.fillText('A',10,40);
    const excludedPixels=context.getImageData(0,0,100,60).data;
    assert(excludedPixels.every((value,index)=>value===fallbackPixels[index]),'excluded glyph really falls back');
    assert(context.measureText('B').width===20,'other included character stays downloaded');
    let invalid=false;
    try{upper.unicodeRange='U+110000';}catch(error){invalid=error.name==='SyntaxError';}
    assert(invalid&&upper.unicodeRange==='U+42','invalid setter preserves descriptor and registry');
    assert(context.measureText('B').width===20,'invalid mutation cannot change valid font bytes');
    const bad=new FontFace('BadRange',bytes,{unicodeRange:'U+FFFF-0'});
    assert(bad.status==='error','malformed construction cannot load');
    let rejected=false;try{await bad.loaded;}catch(error){rejected=error.name==='SyntaxError';}
    assert(rejected,'invalid descriptor rejects loaded promise');
    const astral=new FontFace('AstralRange','url(/unrequested.woff)',{unicodeRange:'U+1F600'});
    set.add(astral);
    assert(!set.check('20px AstralRange','😀'),'query uses supplementary codepoints');
    assert(set.check('20px AstralRange','😃'),'adjacent supplementary codepoint excluded');
    set.delete(astral);set.delete(upper);set.delete(lower);
    assert(context.measureText('B').width!==20,'deleted subset cannot linger in provider');
    return 'passed';
}
