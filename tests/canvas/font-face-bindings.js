async function testFontFaceBindings(bytes) {
    const assert=(v,m)=>{if(!v)throw Error(m);};
    const throws=(callback,name,label)=>{
        let error;try{callback();}catch(value){error=value;}
        assert(error?.name===name,label+': '+error);
    };
    throws(()=>new FontFace(),'TypeError','required arguments');
    throws(()=>new FontFace(Symbol(),'url(/font.ttf)'),'TypeError','family DOMString');
    throws(()=>new FontFace('Family',Symbol()),'TypeError','source DOMString');
    throws(()=>new FontFace('Family',bytes,42),'TypeError','dictionary type');
    throws(()=>new FontFace('Family',bytes,{weight:Symbol()}),'TypeError','descriptor DOMString');
    const order=[];
    const descriptors=new Proxy({},{
        get(_,name){order.push(name);return undefined;}
    });
    const converted=new FontFace({toString(){order.push('family');return 'BindingOrder';}},
        {toString(){order.push('source');return 'url(/font.ttf)';}},descriptors);
    assert(converted.status==='unloaded','source union string conversion');
    // Engines can implement additional descriptor dictionary members. Verify
    // this shared Level 3 contract without pretending those extensions exist here.
    const known=['family','source','ascentOverride','descentOverride','display','featureSettings',
        'lineGapOverride','stretch','style','unicodeRange','variationSettings','weight','width'];
    assert(order.filter(name=>known.includes(name)).join('|')===known.join('|'),
        'arguments left-to-right; dictionary members once and sorted: '+order.join('|'));
    let reads=0;
    const face=new FontFace('BindingGetter',bytes,{get weight(){reads++;return '400';}});
    await face.load();
    assert(reads===1&&face.status==='loaded','descriptor getter read once');
    const padded=new Uint8Array(bytes.byteLength+16);padded.set(bytes,7);
    const view=new DataView(padded.buffer,7,bytes.byteLength);
    for(const name of ['buffer','byteOffset','byteLength'])
        Object.defineProperty(view,name,{get(){throw Error('author '+name);}});
    const dataFace=new FontFace('BindingDataView',view);await dataFace.load();
    assert(dataFace.status==='loaded','DataView uses intrinsic offset and length');
    const typed=new Uint8Array(padded.buffer,7,bytes.byteLength);
    Object.setPrototypeOf(typed,null);
    for(const name of ['buffer','byteOffset','byteLength'])
        Object.defineProperty(typed,name,{get(){throw Error('author '+name);}});
    const typedFace=new FontFace('BindingTypedArray',typed);await typedFace.load();
    assert(typedFace.status==='loaded','typed-array brand survives author prototype changes');
    const owned=bytes.slice();
    const copied=new FontFace('BindingSnapshot',owned);
    owned.fill(0);await copied.load();
    assert(copied.status==='loaded','constructor snapshots actual font bytes');
    const resized=new ArrayBuffer(4,{maxByteLength:8});
    throws(()=>new FontFace('BindingResizable',resized),'TypeError','resizable buffer conversion');
    throws(()=>new FontFace('BindingResizableView',new Uint8Array(resized)),'TypeError','resizable view conversion');
    if(typeof SharedArrayBuffer==='function') {
        const shared=new SharedArrayBuffer(4);
        throws(()=>new FontFace('BindingShared',shared),'TypeError','shared buffer conversion');
        throws(()=>new FontFace('BindingSharedView',new Uint8Array(shared)),'TypeError','shared view conversion');
    }
    const detached=new ArrayBuffer(4);
    structuredClone(detached,{transfer:[detached]});
    const empty=new FontFace('BindingDetached',detached);
    let failed=false;await empty.load().catch(error=>{failed=error.name==='SyntaxError';});
    assert(failed&&empty.status==='error','detached source cannot decode as a font');
    let invalidLoad=false;
    await FontFace.prototype.load.call({}).catch(error=>{invalidLoad=error.name==='TypeError';});
    assert(invalidLoad,'Promise-returning load rejects an invalid receiver');
    const family=Object.getOwnPropertyDescriptor(FontFace.prototype,'family');
    throws(()=>family.get.call({}),'TypeError','descriptor getter receiver brand');
    throws(()=>family.set.call(face,Symbol()),'TypeError','descriptor setter DOMString');
    assert(face.family==='BindingGetter','failed setter preserves previous value');
    const set=typeof document==='object'?document.fonts:fonts;
    for(const name of ['_syncCSSFaces','_fontLoading','_fontSettled','_finishLoading'])
        assert(!(name in FontFaceSet.prototype),'platform helper is not a public method: '+name);
    for(const name of ['_syncCSSFaces','_fontLoading','_fontSettled','_finishLoading'])
        Object.defineProperty(set,name,{value(){throw Error('author lifecycle helper');},configurable:true});
    const originalGet=WeakMap.prototype.get;
    try {
        WeakMap.prototype.get=()=>{throw Error('author WeakMap getter');};
        assert(face.family==='BindingGetter','font state uses captured WeakMap intrinsic');
    } finally {WeakMap.prototype.get=originalGet;}
    const privateFace=new FontFace('BindingPrivateLifecycle',bytes);
    privateFace.load=()=>Promise.reject(Error('author load'));
    set.add(privateFace);
    await privateFace.loaded;
    assert(set.check('20px BindingPrivateLifecycle'),'automatic loading ignores an author load override');
    Object.defineProperty(privateFace,'status',{value:'unloaded'});
    assert(set.check('20px BindingPrivateLifecycle'),'check observes the private loading state');
    set.delete=()=>{throw Error('author delete');};
    FontFaceSet.prototype.clear.call(set);
    assert(!set.has(privateFace),'clear uses platform deletion rather than author method');
    delete set.delete;
    let invalidSetLoad=false;
    await FontFaceSet.prototype.load.call({},'20px Family').catch(error=>{invalidSetLoad=error.name==='TypeError';});
    assert(invalidSetLoad,'FontFaceSet.load rejects invalid receiver');
    throws(()=>set.check(),'TypeError','check required argument');
    throws(()=>set.check(Symbol()),'TypeError','check DOMString conversion');
    throws(()=>set.has({}),'TypeError','has FontFace interface argument');
    let textConverted=0;
    set.check('20px MissingFace',{toString(){textConverted++;return 'A';}});
    assert(textConverted===1,'check text DOMString conversion');
    for(const name of ['_syncCSSFaces','_fontLoading','_fontSettled','_finishLoading'])delete set[name];
    return 'passed';
}
