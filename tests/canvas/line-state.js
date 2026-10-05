function testCanvasLineState(createCanvas) {
    const canvas=createCanvas();canvas.width=32;canvas.height=16;
    const c=canvas.getContext('2d'),assert=(condition,label)=>{if(!condition)throw Error(label);};
    const errors=[],fail=fn=>{try{fn();throw Error('missing exception');}catch(error){
        if(!(error instanceof TypeError))throw error;errors.push(error.name);}};
    for(const name of ['lineWidth','miterLimit','lineDashOffset']) {
        c[name]=7;let calls=0;c[name]={valueOf(){calls++;return 3;}};
        assert(c[name]===3&&calls===1,'numeric conversion '+name);
        for(const value of [NaN,Infinity,-Infinity])c[name]=value;
        assert(c[name]===3,'nonfinite unchanged '+name);
        fail(()=>{c[name]=1n;});fail(()=>{c[name]=Symbol();});
        const descriptor=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(c),name);
        assert(descriptor.enumerable&&descriptor.configurable,'attribute descriptor '+name);
        let touched=false;
        fail(()=>descriptor.set.call({}, {valueOf(){touched=true;return 2;}}));
        fail(()=>descriptor.get.call({}));assert(!touched,'receiver before conversion');
    }
    c.lineWidth=0;c.miterLimit=-1;assert(c.lineWidth===3&&c.miterLimit===3,'positive-only state');
    c.lineDashOffset=-2;assert(c.lineDashOffset===-2,'signed offset');
    let strings=0;c.lineCap={toString(){strings++;return 'round';}};
    c.lineJoin={toString(){strings++;return 'bevel';}};
    assert(strings===2&&c.lineCap==='round'&&c.lineJoin==='bevel','enum DOMString conversion');
    c.lineCap='bogus';c.lineJoin='bogus';assert(c.lineCap==='round'&&c.lineJoin==='bevel','invalid enum ignored');
    fail(()=>{c.lineCap=Symbol();});fail(()=>{c.lineJoin=Symbol();});
    const trace=[];let index=0,nextReads=0;
    const iterable={ [Symbol.iterator](){trace.push('iterator');return {
        get next(){nextReads++;return function(){trace.push('next'+index);return index++<2?
            {done:false,value:{valueOf(){trace.push('number'+index);return index;}}}:{done:true};};}
    };}};
    c.setLineDash(iterable);
    assert(JSON.stringify(trace)==='["iterator","next0","number1","next1","number2","next2"]'&&nextReads===1,
        'iterator and per-value conversion ordering');
    assert(JSON.stringify(c.getLineDash())==='[1,2]','converted dash sequence');
    let returns=0,nextCalls=0;
    fail(()=>c.setLineDash({[Symbol.iterator](){return {next(){nextCalls++;return {value:1n,done:false};},
        return(){returns++;return {done:true};}};}}));
    assert(nextCalls===1&&returns===0&&JSON.stringify(c.getLineDash())==='[1,2]','abrupt sequence preserves state');
    for(const value of [undefined,null,'12',{},1])fail(()=>c.setLineDash(value));
    fail(()=>c.setLineDash());fail(()=>c.setLineDash.call({},[]));fail(()=>c.getLineDash.call({}));
    fail(()=>c.setLineDash({[Symbol.iterator](){return {next(){return 1;}};}}));
    c.setLineDash([2,3,4]);const copy=c.getLineDash();copy[0]=99;
    assert(JSON.stringify(c.getLineDash())==='[2,3,4,2,3,4]','odd duplication and owned returned sequence');
    for(const invalid of [[1,-1],[Infinity,1],[NaN,1]])c.setLineDash(invalid);
    assert(c.getLineDash()[0]===2,'invalid algorithm input preserves prior state');
    c.save();c.setLineDash([]);c.lineWidth=8;c.restore();
    assert(c.lineWidth===3&&c.getLineDash().length===6,'saved line state');
    c.setLineDash([4,4]);c.lineCap='butt';c.lineWidth=2;c.lineDashOffset=0;
    c.strokeStyle='red';c.beginPath();c.moveTo(0,8);c.lineTo(32,8);c.stroke();
    const sample=x=>Array.from(c.getImageData(x,8,1,1).data);
    assert(sample(1)[0]===255&&sample(6)[3]===0,'state drives actual dashed pixels');
    return {trace,errors,samples:[sample(1),sample(6)],dash:c.getLineDash()};
}
