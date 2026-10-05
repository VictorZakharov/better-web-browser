    // OES_draw_buffers_indexed (WebGL2 only), shared by Window and Worker.
    const webGlIndexedBlendBrands=webGlPrivateBrands();
    class OES_draw_buffers_indexed {
        constructor(token,context) {
            if(token!==webGlToken)throw new TypeError('Illegal constructor');
            webGlIndexedBlendBrands.set(this,{context,epoch:webGlState(context).epoch});
        }
    }
    const webGlIndexedBlendSignatures={
        enableiOES:'uu', disableiOES:'uu', blendEquationiOES:'uu',
        blendEquationSeparateiOES:'uuu', blendFunciOES:'uuu',
        blendFuncSeparateiOES:'uuuuu', colorMaskiOES:'ubbbb'
    };
    for(const [name,signature] of Object.entries(webGlIndexedBlendSignatures)) {
        const method=function(...args) {
            const context=webGlExtensionReceiver(this,webGlIndexedBlendBrands,
                signature.length,args,name);
            const values=[];
            // Convert left-to-right even for a lost/retained extension. GLenum
            // and GLuint follow unsigned-long wrapping, booleans use truthiness.
            for(let index=0;index<signature.length;index++)
                values.push(signature[index]==='b'?Number(Boolean(args[index])):
                    webGlScalar('u',args[index]));
            if(context)webGlCall(context,name,values);
        };
        Object.defineProperties(method,{name:{value:name},length:{value:signature.length}});
        Object.defineProperty(OES_draw_buffers_indexed.prototype,name,
            {value:method,writable:true,enumerable:true,configurable:true});
    }
    Object.defineProperty(OES_draw_buffers_indexed.prototype,Symbol.toStringTag,
        {value:'OES_draw_buffers_indexed'});
    webGlExtensionFactories.set('oes_draw_buffers_indexed',{
        name:'OES_draw_buffers_indexed',create:context=>new OES_draw_buffers_indexed(webGlToken,context)
    });
