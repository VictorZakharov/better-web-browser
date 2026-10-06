function installFontTaskProbe(bytes, record) {
    const face=new FontFace('TaskOwnedAhem',bytes), set=typeof document==='object'?document.fonts:fonts;
    set.add(face);
    const loaded=face.loaded;
    if(face.load()!==loaded||face.load()!==loaded)throw Error('load must return the existing loaded promise');
    record('sync:'+face.status);
    Promise.resolve().then(()=>record('micro:'+face.status));
    set.addEventListener('loading',event=>{
        if(!(event instanceof FontFaceSetLoadEvent)||event.fontfaces.length)throw Error('loading event interface');
        record('loading:'+face.status+':'+event.isTrusted);
        set.ready.then(()=>record('ready:'+set.status));
    });
    face.loaded.then(()=>{
        if(face.load()!==loaded)throw Error('loaded face retains promise identity');
        const canvas=typeof document==='object'?document.createElement('canvas'):new OffscreenCanvas(80,60);
        const context=canvas.getContext('2d');context.font='20px TaskOwnedAhem';
        if(context.measureText('ABC').width!==60)throw Error('completed font task installs real advances');
        record('loaded:'+face.status);
    });
    set.addEventListener('loadingdone',event=>{
        if(event.fontfaces[0]!==face||!Object.isFrozen(event.fontfaces))throw Error('completion event faces');
        record('loadingdone:'+set.status+':'+event.isTrusted+':'+event.fontfaces.length);
    });
    // Private font tasks cannot be canceled through timer APIs or replaced by
    // author setTimeout/queueMicrotask overrides after their initial scheduling.
    for(let id=1;id<8;id++)clearTimeout(id);
    globalThis.setTimeout=globalThis.queueMicrotask=()=>{throw Error('author task override');};
    if('__fontLoadingQueue' in globalThis)throw Error('font scheduler handoff leaked');
}
