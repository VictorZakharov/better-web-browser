    for (const [name, signature] of [['bindBufferBase','uu-'], ['bindBufferRange','uu-ll']]) {
        webGl2Method(name, signature.length, signature, [[2,'WebGLBuffer',true]], function(...args) {
            const id = webGl2Handle(this,args[2],'WebGLBuffer',true);
            if (id < 0) return;
            args[2] = id;
            webGl2Invoke(this,name,args.slice(0,signature.length));
        });
    }
    webGl2Method('getIndexedParameter',2,'uu',[],function(target,index) {
        const result = webGlCall(this,'getIndexedParameter',[target,index]);
        return [0x8a28,0x8c8f].includes(target) ? webGlObject(this,'WebGLBuffer',result) : result;
    },null);
    webGl2Method('copyBufferSubData',5,'uulll',[],function(...args) {
        webGl2Invoke(this,'copyBufferSubData',args.slice(0,5));
    });
    webGl2Method('getBufferSubData',3,'ul-au',[[2,webGl2View]],function(target,offset,destination,dstOffset,length) {
        const bytes = webGl2Slice(this,destination,dstOffset,length);
        if (!bytes) return;
        const result = webGlCall(this,'getBufferSubData',[target,offset,bytes.byteLength]);
        if (result) Reflect.apply(webGl2ByteSet,bytes,[result]);
    });
    const webGl2DataArgument = value => {
        if (value === null || typeof value === 'object') {
            // A boxed Number is the size overload, not a forged BufferSource.
            if (value !== null && !webGl2IsView(value)) {
                try { return webGl2Source(value); } catch (error) {
                    try { Reflect.apply(Number.prototype.valueOf,value,[]); } catch { throw error; }
                }
            } else return webGl2Source(value);
        }
        return webGlLongLong(value);
    };
    webGl2Method('bufferData',3,count => count >= 4 ? 'u-uau' : 'u-u',
        count => [[1,count >= 4 ? webGl2View : webGl2DataArgument]],function(target,data,usage,offset=0,length=0) {
        if (typeof data === 'number') { webGl2Invoke(this,'bufferData',[target,data,usage]); return; }
        const bytes = webGl2Slice(this,data,offset,length);
        if (bytes) webGl2Invoke(this,'bufferData',[target,bytes.byteLength,usage],[], '',bytes);
    });
    const webGl2SubDataArgument = value => {
        if (value === null) throw new TypeError('Expected a BufferSource');
        return webGl2Source(value);
    };
    webGl2Method('bufferSubData',3,count => count >= 4 ? 'ul-au' : 'ul-',
        count => [[2,count >= 4 ? webGl2View : webGl2SubDataArgument]],function(target,offset,data,srcOffset=0,length=0) {
        const bytes = webGl2Slice(this,data,srcOffset,length);
        if (bytes) webGl2Invoke(this,'bufferSubData',[target,offset],[], '',bytes);
    });
