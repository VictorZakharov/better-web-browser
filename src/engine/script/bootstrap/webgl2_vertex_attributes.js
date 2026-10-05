    webGl2Method('vertexAttribIPointer',5,'uiuil',[],function(...args) {
        webGl2Invoke(this,'vertexAttribIPointer',args.slice(0,5));
    });
    for (const [kind,suffix] of [['i','i'],['u','ui']]) {
        const scalar = 'vertexAttribI4'+suffix;
        webGl2Method(scalar,5,'u'+kind.repeat(4),[],function(...args) {
            webGl2Invoke(this,scalar,args.slice(0,5));
        });
        webGl2Method(scalar+'v',2,'u-',[[1,webGl2NumericArgument(kind)]],function(index,source) {
            if (source.length < 4) { webGlError(this,0x0501); return; }
            const values = webGl2NumericSlice(this,source,0,4);
            if (values) webGl2Invoke(this,scalar+'v',[index,...values]);
        });
    }
