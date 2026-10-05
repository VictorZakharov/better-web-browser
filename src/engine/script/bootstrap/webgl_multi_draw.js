    // Khronos WEBGL_multi_draw: owned lists, ordered Web IDL conversion, and
    // one genuine native multi-draw call (not gl_DrawID-breaking draw loops).
    const webGlMultiDrawBrands = webGlPrivateBrands();
    const webGlMultiIntList = value => {
        if (webGl2IsView(value) && Reflect.apply(webGl2TypedName,value,[]) === 'Int32Array') {
            const view=webGl2View(value);
            if(view.length>1048576)throw new RangeError('WebGL typed list exceeds the snapshot budget');
            const source=new webGl2NumericConstructors.i(view.buffer,view.byteOffset,view.length);
            const owned=new webGl2NumericConstructors.i(view.length);
            // The Int32Array/sequence union snapshots at argument conversion,
            // matching Chrome even when later getters mutate the original.
            // Captured typed-array set ignores author iterators/properties.
            Reflect.apply(webGl2ByteSet,owned,[source]);
            return {values:owned,length:view.length};
        }
        const values=webGl2Sequence(value,'i');
        return {values,length:values.length};
    };
    const webGlMultiSubmit = (context, name, mode, kind, starts, startOffset,
        counts, countOffset, instances, instanceOffset, drawcount) => {
        if (!context) return;
        if (drawcount < 0) {webGlError(context,0x0501);return;}
        for(const [list,offset] of [[starts,startOffset],[counts,countOffset],
            ...(instances?[[instances,instanceOffset]]:[])]) {
            if(!Number.isSafeInteger(offset)||offset>list.length||drawcount>list.length-offset) {
                webGlError(context,0x0501);return;
            }
        }
        if(drawcount>4096){webGlError(context,0x0505);return;}
        const values=[mode,kind,drawcount];
        // Serialize only the requested slices, from owned conversion snapshots.
        // Shared backing stores cannot be read while native validation executes.
        for(let i=0;i<drawcount;i++)values.push(starts.values[startOffset+i],counts.values[countOffset+i],
            instances?instances.values[instanceOffset+i]:1);
        webGlCall(context,name,values);
        webGlState(context).dirty=true;
        dirtyWebGlCanvas(context);
    };
    class WEBGL_multi_draw {
        constructor(token,context) {
            if(token!==webGlToken)throw new TypeError('Illegal constructor');
            webGlMultiDrawBrands.set(this,{context,epoch:webGlState(context).epoch});
        }
        multiDrawArraysWEBGL(mode,firsts,firstsOffset,counts,countsOffset,drawcount) {
            const context=webGlExtensionReceiver(this,webGlMultiDrawBrands,6,arguments,'multiDrawArraysWEBGL');
            mode=webGlScalar('u',mode);firsts=webGlMultiIntList(firsts);firstsOffset=webGl2UnsignedLongLong(firstsOffset);
            counts=webGlMultiIntList(counts);countsOffset=webGl2UnsignedLongLong(countsOffset);drawcount=webGlScalar('i',drawcount);
            webGlMultiSubmit(context,'multiDrawArraysWEBGL',mode,0,firsts,firstsOffset,counts,countsOffset,null,0,drawcount);
        }
        multiDrawElementsWEBGL(mode,counts,countsOffset,type,offsets,offsetsOffset,drawcount) {
            const context=webGlExtensionReceiver(this,webGlMultiDrawBrands,7,arguments,'multiDrawElementsWEBGL');
            mode=webGlScalar('u',mode);counts=webGlMultiIntList(counts);countsOffset=webGl2UnsignedLongLong(countsOffset);
            type=webGlScalar('u',type);offsets=webGlMultiIntList(offsets);offsetsOffset=webGl2UnsignedLongLong(offsetsOffset);drawcount=webGlScalar('i',drawcount);
            webGlMultiSubmit(context,'multiDrawElementsWEBGL',mode,type,offsets,offsetsOffset,counts,countsOffset,null,0,drawcount);
        }
        multiDrawArraysInstancedWEBGL(mode,firsts,firstsOffset,counts,countsOffset,instances,instancesOffset,drawcount) {
            const context=webGlExtensionReceiver(this,webGlMultiDrawBrands,8,arguments,'multiDrawArraysInstancedWEBGL');
            mode=webGlScalar('u',mode);firsts=webGlMultiIntList(firsts);firstsOffset=webGl2UnsignedLongLong(firstsOffset);
            counts=webGlMultiIntList(counts);countsOffset=webGl2UnsignedLongLong(countsOffset);
            instances=webGlMultiIntList(instances);instancesOffset=webGl2UnsignedLongLong(instancesOffset);drawcount=webGlScalar('i',drawcount);
            webGlMultiSubmit(context,'multiDrawArraysInstancedWEBGL',mode,0,firsts,firstsOffset,counts,countsOffset,instances,instancesOffset,drawcount);
        }
        multiDrawElementsInstancedWEBGL(mode,counts,countsOffset,type,offsets,offsetsOffset,instances,instancesOffset,drawcount) {
            const context=webGlExtensionReceiver(this,webGlMultiDrawBrands,9,arguments,'multiDrawElementsInstancedWEBGL');
            mode=webGlScalar('u',mode);counts=webGlMultiIntList(counts);countsOffset=webGl2UnsignedLongLong(countsOffset);
            type=webGlScalar('u',type);offsets=webGlMultiIntList(offsets);offsetsOffset=webGl2UnsignedLongLong(offsetsOffset);
            instances=webGlMultiIntList(instances);instancesOffset=webGl2UnsignedLongLong(instancesOffset);drawcount=webGlScalar('i',drawcount);
            webGlMultiSubmit(context,'multiDrawElementsInstancedWEBGL',mode,type,offsets,offsetsOffset,counts,countsOffset,instances,instancesOffset,drawcount);
        }
    }
    Object.defineProperty(WEBGL_multi_draw.prototype,Symbol.toStringTag,{value:'WEBGL_multi_draw'});
    for(const name of ['multiDrawArraysWEBGL','multiDrawElementsWEBGL',
        'multiDrawArraysInstancedWEBGL','multiDrawElementsInstancedWEBGL'])
        Object.defineProperty(WEBGL_multi_draw.prototype,name,{enumerable:true});
    webGlExtensionFactories.set('webgl_multi_draw',{
        name:'WEBGL_multi_draw',create:context=>new WEBGL_multi_draw(webGlToken,context)
    });
