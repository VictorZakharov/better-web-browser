function testPatternContracts(createCanvas) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const make=(w,h)=>{const c=createCanvas();c.width=w;c.height=h;return c;};
    const source=make(2,1),s=source.getContext('2d');
    s.fillStyle='red';s.fillRect(0,0,1,1);s.fillStyle='blue';s.fillRect(1,0,1,1);
    const target=make(8,4),c=target.getContext('2d');c.imageSmoothingEnabled=false;
    const pattern=c.createPattern(source,'repeat');
    assert(Object.prototype.toString.call(pattern)==='[object CanvasPattern]','pattern brand');
    assert(pattern.setTransform.length===0&&c.createPattern.length===2,'IDL arities');
    const descriptor=Object.getOwnPropertyDescriptor(CanvasPattern.prototype,'setTransform');
    assert(descriptor.enumerable&&descriptor.writable&&descriptor.configurable,'method descriptor');
    const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data).join();
    const errorName=callback=>{try{callback();return 'none';}catch(error){return error.name;}};
    const errors=[errorName(()=>CanvasPattern.prototype.setTransform.call({})),
        errorName(()=>c.createPattern(source)),errorName(()=>c.createPattern(source,undefined)),
        errorName(()=>c.createPattern(source,Symbol())),errorName(()=>pattern.setTransform('matrix(1,0,0,1,0,0)')),
        errorName(()=>pattern.setTransform({a:1,m11:2})),errorName(()=>pattern.setTransform({a:1n})),
        errorName(()=>c.createPattern(make(0,1),'repeat'))];
    assert(errors.join()==='TypeError,TypeError,SyntaxError,TypeError,TypeError,TypeError,TypeError,InvalidStateError','IDL errors '+errors);
    const trace=[];const dictionary={};
    for(const name of ['a','b','c','d','e','f','m11','m12','m21','m22','m41','m42'])
        Object.defineProperty(dictionary,name,{get(){trace.push(name);return undefined;}});
    Object.defineProperty(dictionary,Symbol.iterator,{get(){throw Error('dictionary iterator accessed');}});
    Object.defineProperty(dictionary,'is2D',{get(){throw Error('unknown dictionary member accessed');}});
    pattern.setTransform(dictionary);
    assert(trace.join()==='a,b,c,d,e,f,m11,m12,m21,m22,m41,m42','ordered single-read dictionary');
    // Opaque implementation state cannot be replaced by author properties.
    pattern.__source={width:1,height:1,pixels:new Uint8ClampedArray([0,255,0,255])};
    pattern.__repetition='no-repeat';pattern.__transform=[0,0,0,0,0,0];
    s.fillStyle='green';s.fillRect(0,0,2,1);source.width=1;
    c.fillStyle=pattern;c.fillRect(0,0,8,1);
    assert(pixel(0,0)==='255,0,0,255'&&pixel(1,0)==='0,0,255,255'&&pixel(6,0)==='255,0,0,255','immutable source snapshot');
    pattern.setTransform({e:1});c.fillRect(0,1,8,1);
    assert(pixel(0,1)==='0,0,255,255'&&pixel(1,1)==='255,0,0,255','live pattern matrix');
    pattern.setTransform({e:Infinity});c.fillRect(0,2,8,1);
    assert(pixel(1,2)==='255,0,0,255','nonfinite leaves prior matrix');
    pattern.setTransform([2,0,0,2,5,5]);c.fillRect(0,3,8,1);
    assert(pixel(0,3)==='255,0,0,255','sequence is empty dictionary, not constructor overload');
    const shifted=c.createPattern(target,null);
    assert(shifted,'legacy null repetition');
    const samples=[pixel(0,0),pixel(1,0),pixel(0,1),pixel(1,1),pixel(1,2),pixel(0,3)];
    return {samples,errors,trace,passed:true};
}
