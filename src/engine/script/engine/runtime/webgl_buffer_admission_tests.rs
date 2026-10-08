//! WebGL BufferSource types admit shared fixed stores, not resizable stores.
use super::webgl_owned_copy_tests::both;

const CONTEXTS: &str = r#"
    for (const api of ['webgl','webgl2']) {
        const gl=new OffscreenCanvas(2,2).getContext(api,{antialias:false});
        if(!gl)throw Error('real native '+api+' unavailable');
        const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([7,8,9,10]),gl.STATIC_DRAW);
        const rejects=call=>{
            let rejected=false;
            try{call();}catch(error){rejected=error instanceof TypeError;}
            if(!rejected)throw Error(api+' did not enforce fixed backing-store admission');
        };
"#;

#[test]
fn webgl_resizable_upload_and_reply_arguments_reject_in_both_native_versions_and_realms() {
    both(&format!(
        r#"{CONTEXTS}
        const storage=new ArrayBuffer(32,{{maxByteLength:64}});
        const views=[new Uint8Array(storage),new DataView(storage)];
        for(const view of views){{
            rejects(()=>gl.bufferData(gl.ARRAY_BUFFER,view,gl.STATIC_DRAW));
            rejects(()=>gl.bufferSubData(gl.ARRAY_BUFFER,0,view));
            rejects(()=>gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,view));
            rejects(()=>gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,view));
            rejects(()=>gl.compressedTexImage2D(gl.TEXTURE_2D,0,0,1,1,0,view));
            if(api==='webgl2'){{
                rejects(()=>gl.bufferData(gl.ARRAY_BUFFER,view,gl.STATIC_DRAW,0,4));
                rejects(()=>gl.bufferSubData(gl.ARRAY_BUFFER,0,view,0,4));
                rejects(()=>gl.getBufferSubData(gl.ARRAY_BUFFER,0,view,0,4));
                rejects(()=>gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,view,1));
            }}
        }}
        rejects(()=>gl.bufferData(gl.ARRAY_BUFFER,storage,gl.STATIC_DRAW));
        rejects(()=>gl.bufferSubData(gl.ARRAY_BUFFER,0,storage));
        if(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==4||gl.getError()!==0)
            throw Error('IDL rejection changed native storage/error state');
        if(api==='webgl2'){{
            const output=new Uint8Array(4);gl.getBufferSubData(gl.ARRAY_BUFFER,0,output);
            if(String(output)!=='7,8,9,10')throw Error('IDL failure changed native bytes');
        }}
    }}
    "#
    ));
}

#[test]
fn webgl_resizable_argument_failure_keeps_conversion_order_even_on_lost_contexts() {
    both(&format!(
        r#"{CONTEXTS}
        const storage=new ArrayBuffer(16,{{maxByteLength:32}}),view=new Uint8Array(storage);
        const events=[];
        const target={{valueOf(){{events.push('target');return gl.ARRAY_BUFFER;}}}};
        const usage={{valueOf(){{events.push('usage');return gl.STATIC_DRAW;}}}};
        for(const lost of [false,true]){{
            if(lost){{
                const extension=gl.getExtension('WEBGL_lose_context');
                if(!extension)throw Error('native context-loss extension unavailable');
                extension.loseContext();
            }}
            events.length=0;
            rejects(()=>gl.bufferData(target,view,usage));
            if(String(events)!=='target')throw Error('resizable conversion did not precede usage');
            events.length=0;
            rejects(()=>gl.bufferData(target,storage,usage));
            if(String(events)!=='target')throw Error('bare buffer conversion did not precede usage');
            rejects(()=>gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,view));
        }}
    }}
    "#
    ));
}

#[test]
fn webgl_backing_store_admission_ignores_author_shadowing_of_resizable_getters() {
    both(&format!(
        r#"{CONTEXTS}
        const storage=new ArrayBuffer(16,{{maxByteLength:32}}),view=new Uint8Array(storage);
        Object.defineProperty(storage,'resizable',{{value:false}});
        Object.defineProperty(view,'buffer',{{value:new ArrayBuffer(16)}});
        const descriptor=Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'resizable');
        const originalApply=Reflect.apply;
        try{{
            Object.defineProperty(ArrayBuffer.prototype,'resizable',{{configurable:true,get(){{return false;}}}});
            // Even replacing the dispatch helper cannot spoof backing-store brands.
            Reflect.apply=(method,receiver,args)=>
                method===descriptor.get ? false : originalApply(method,receiver,args);
            rejects(()=>gl.bufferData(gl.ARRAY_BUFFER,storage,gl.STATIC_DRAW));
            rejects(()=>gl.bufferData(gl.ARRAY_BUFFER,view,gl.STATIC_DRAW));
        }}finally{{Reflect.apply=originalApply;Object.defineProperty(ArrayBuffer.prototype,'resizable',descriptor);}}
        if(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==4||gl.getError()!==0)
            throw Error('shadowed backing-store admission mutated native state');
    }}
    "#
    ));
}

#[test]
fn webgl_byte_readback_uses_intrinsic_destination_ranges_and_not_author_constructors() {
    both(
        r#"
        for(const api of ['webgl','webgl2']){
            const gl=new OffscreenCanvas(2,2).getContext(api,{antialias:false});
            if(!gl)throw Error('real native '+api+' unavailable');
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const Bytes=Uint8Array,backing=new Bytes(12);backing.fill(83);
            const destination=backing.subarray(4,8),decoy=new ArrayBuffer(12);
            Object.defineProperties(destination,{
                buffer:{value:decoy},byteLength:{value:12},byteOffset:{value:0}
            });
            try{
                globalThis.Uint8Array=function(){throw Error('author byte constructor invoked');};
                gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,destination);
            }finally{globalThis.Uint8Array=Bytes;}
            if(String(backing)!=='83,83,83,83,255,0,0,255,83,83,83,83')
                throw Error(api+' widened or missed intrinsic destination range');
            if(new Bytes(decoy).some(value=>value!==0))throw Error('author decoy buffer was written');
            if(gl.getError()!==0)throw Error('native byte readback error');
        }
        "#,
    );
}
