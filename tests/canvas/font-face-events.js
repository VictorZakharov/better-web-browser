function testFontFaceEvents() {
    const assert=(value,message)=>{if(!value)throw Error(message);};
    const typeError=callback=>{
        let error;try{callback();}catch(caught){error=caught;}
        assert(error instanceof TypeError,'expected TypeError');
    };
    const first=new FontFace('EventFirst','url(first.woff2)');
    const second=new FontFace('EventSecond','url(second.woff2)');
    typeError(()=>new FontFaceSetLoadEvent());
    typeError(()=>new FontFaceSetLoadEvent(Symbol()));
    for(const value of [false,1,'dictionary'])
        typeError(()=>new FontFaceSetLoadEvent('done',value));
    const order=[];
    const dictionary=Object.create({
        get bubbles(){order.push('bubbles');return true;},
        get cancelable(){order.push('cancelable');return true;},
        get composed(){order.push('composed');return false;},
        get fontfaces(){order.push('fontfaces');return [first,second,first];}
    });
    const event=new FontFaceSetLoadEvent({toString(){order.push('type');return 'done';}},dictionary);
    assert(order.join(',')==='type,bubbles,cancelable,composed,fontfaces','dictionary conversion order');
    assert(event.type==='done'&&event.bubbles&&event.cancelable,'EventInit flags retained');
    assert(event instanceof Event&&event.fontfaces.length===3,'event inheritance and duplicates');
    assert(event.fontfaces[0]===first&&event.fontfaces[1]===second,'FontFace identity');
    assert(event.fontfaces===event.fontfaces&&Object.isFrozen(event.fontfaces),'stable frozen sequence');
    assert(!Object.hasOwn(event,'fontfaces'),'attribute belongs to prototype');
    const descriptor=Object.getOwnPropertyDescriptor(FontFaceSetLoadEvent.prototype,'fontfaces');
    assert(descriptor.enumerable&&descriptor.configurable&&!descriptor.set,'readonly IDL attribute');
    typeError(()=>descriptor.get.call({}));
    typeError(()=>{'use strict';event.fontfaces=[];});
    typeError(()=>event.fontfaces.push(first));
    const source=[first];
    const snapshot=new FontFaceSetLoadEvent('snapshot',{fontfaces:source});
    source[0]=second;
    assert(snapshot.fontfaces[0]===first,'sequence copy is independent');
    for(const value of [null,1,'not a sequence',{},[{}],[Object.create(FontFace.prototype)]])
        typeError(()=>new FontFaceSetLoadEvent('invalid',{fontfaces:value}));
    assert(new FontFaceSetLoadEvent('empty',null).fontfaces.length===0,'null dictionary defaults');
    assert(new FontFaceSetLoadEvent('empty',{fontfaces:undefined}).fontfaces.length===0,'undefined member defaults');
    let closed=0,nextGets=0,valueGets=0;
    const iterable={
        [Symbol.iterator](){return {
            get next(){nextGets++;return ()=>({done:false,get value(){valueGets++;return {};}});},
            return(){closed++;return {done:true};}
        };}
    };
    typeError(()=>new FontFaceSetLoadEvent('invalid',{fontfaces:iterable}));
    assert(nextGets===1&&valueGets===1&&closed===0,'Web IDL failure does not close iterator');
    let ignoredValue=false;
    const completed={ [Symbol.iterator](){return {next(){return {
        done:true,get value(){ignoredValue=true;throw Error('unused');}
    };}};}};
    assert(new FontFaceSetLoadEvent('empty',{fontfaces:completed}).fontfaces.length===0&&!ignoredValue,
        'completed iterator does not access value');
    const sentinel={};let caught;
    try{new FontFaceSetLoadEvent('throwing',{get bubbles(){throw sentinel;},get fontfaces(){throw Error('late');}});}
    catch(error){caught=error;}
    assert(caught===sentinel,'original dictionary exception propagates');
    const oldFreeze=Object.freeze,oldFrom=Array.from;
    try{
        Object.freeze=Array.from=()=>{throw Error('author replacement');};
        assert(new FontFaceSetLoadEvent('private',{fontfaces:[first]}).fontfaces[0]===first,
            'conversion uses captured platform intrinsics');
    }finally{Object.freeze=oldFreeze;Array.from=oldFrom;}
    return 'passed';
}
