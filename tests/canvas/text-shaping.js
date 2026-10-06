function testCanvasTextShaping(make) {
    const assert=(condition,label)=>{if(!condition)throw Error(label);};
    const canvas=make(420,100), context=canvas.getContext('2d');
    assert(context.fontKerning==='auto' && context.lang==='inherit','initial text shaping');
    for(const name of ['fontKerning','lang']) {
        const descriptor=Object.getOwnPropertyDescriptor(CanvasRenderingContext2D.prototype,name);
        assert(descriptor.enumerable && descriptor.configurable,'shaping IDL descriptor');
        let converted=false,error;
        try{descriptor.set.call({}, {toString(){converted=true;return 'normal';}});}
        catch(caught){error=caught;}
        assert(error instanceof TypeError && !converted,'shaping receiver before conversion');
        error=undefined;
        try{context[name]=Symbol();}catch(caught){error=caught;}
        assert(error instanceof TypeError,'shaping Symbol rejects');
    }
    context.fontKerning='normal';
    for(const invalid of ['NORMAL','invalid','',null,undefined]) {
        context.fontKerning=invalid;
        assert(context.fontKerning==='normal','invalid kerning enum ignored');
    }
    const hints=[];
    context.lang={[Symbol.toPrimitive](hint){hints.push(hint);return 'sr-Latn';}};
    assert(context.lang==='sr-Latn' && hints.join()==='string','language DOMString');
    context.font='48px Arial';
    context.fontKerning='normal'; const kerned=context.measureText('AVATAR').width;
    context.fontKerning='none'; const unkerned=context.measureText('AVATAR').width;
    assert(unkerned>kerned+0.1,'kern feature changes actual advances');
    context.fontKerning='auto';
    assert(Math.abs(context.measureText('AVATAR').width-kerned)<0.001,'auto uses provider kerning');
    context.fontKerning='none';context.save();
    context.fontKerning='normal';context.lang='ru';context.direction='rtl';
    context.restore();
    assert(context.fontKerning==='none' && context.lang==='sr-Latn' && context.direction==='inherit',
        'shaping state restored together');
    context.font='24px Arial';context.textAlign='left';context.textBaseline='alphabetic';
    const pixels=()=>Array.from(context.getImageData(0,0,420,100).data);
    context.direction='ltr';context.fillText('ABC \u05d0\u05d1\u05d2',10,40);
    const ltr=pixels();context.clearRect(0,0,420,100);
    context.direction='rtl';context.fillText('ABC \u05d0\u05d1\u05d2',10,40);
    const rtl=pixels();
    assert(ltr.some((value,index)=>value!==rtl[index]),'base direction changes mixed-script glyph order');
    assert(ltr.some(value=>value) && rtl.some(value=>value),'both direction runs paint actual glyphs');
    canvas.width=canvas.width;
    assert(context.fontKerning==='auto' && context.lang==='inherit','resize resets shaping');
}
