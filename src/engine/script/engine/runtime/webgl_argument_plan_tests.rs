//! Precompiled IDL metadata must not cache converted values or author state.
use super::webgl_owned_copy_tests::both;

#[test]
fn webgl_fixed_conversion_slots_ignore_replaced_metadata_search_helpers() {
    both(
        r#"
        for(const api of ['webgl','webgl2']) {
            const gl=new OffscreenCanvas(8,4).getContext(api,{antialias:false});
            if(!gl)throw Error('native context unavailable');
            const buffer=gl.createBuffer();
            const get=Map.prototype.get,find=Array.prototype.find;
            let hooks=0,converted=0;
            const fail=()=>{hooks++;throw Error('private IDL metadata consulted author helper')};
            Map.prototype.get=Array.prototype.find=fail;
            try {
                gl.bindBuffer({valueOf(){converted++;return gl.ARRAY_BUFFER}},buffer);
                gl.viewport(0,0,8,4);
                gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
                gl.bindBuffer(gl.ARRAY_BUFFER,null);
            } finally {Map.prototype.get=get;Array.prototype.find=find;}
            if(hooks||converted!==1)throw Error('metadata lookup or conversion changed');
            const pixel=new Uint8Array(4);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if(String(pixel)!=='0,255,0,255'||gl.getError()!==0)throw Error('native result');
        }
    "#,
    );
}

#[test]
fn webgl_argument_plans_preserve_conversion_order_reentrancy_and_extra_arguments() {
    both(
        r#"
        for(const api of ['webgl','webgl2']) {
            const gl=new OffscreenCanvas(8,4).getContext(api),order=[];
            const number=(name,value)=>({valueOf(){order.push(name);return value}});
            gl.viewport(number('x',1),number('y',2),number('w',6),number('h',2),
                {valueOf(){throw Error('extra argument was converted')}});
            if(String(order)!=='x,y,w,h'||String(gl.getParameter(gl.VIEWPORT))!=='1,2,6,2')
                throw Error('scalar slots changed');
            order.length=0;
            let rejected=false;
            try {gl.bindBuffer(number('target',gl.ARRAY_BUFFER),{});}
            catch(error){rejected=error instanceof TypeError;}
            if(!rejected||String(order)!=='target'||gl.getError()!==0)
                throw Error('interface conversion order');
            order.length=0;
            gl.viewport({valueOf(){
                order.push('outer');gl.viewport(2,1,4,3);return 0;
            }},number('y',0),number('w',8),number('h',4));
            if(String(order)!=='outer,y,w,h'||String(gl.getParameter(gl.VIEWPORT))!=='0,0,8,4')
                throw Error('shared plan retained nested argument values');
            for(const call of [()=>gl.clear(1n),()=>gl.viewport(Symbol(),0,1,1)]) {
                rejected=false;try{call()}catch(error){rejected=error instanceof TypeError}
                if(!rejected)throw Error('invalid scalar accepted');
            }
            if(gl.getError()!==0)throw Error('IDL exception became native error');
        }
    "#,
    );
}

#[test]
fn webgl_argument_plans_do_not_reuse_another_context_or_skip_conversion_after_loss() {
    both(
        r#"
        for(const api of ['webgl','webgl2']) {
            const gl=new OffscreenCanvas(4,4).getContext(api);
            const peer=new OffscreenCanvas(4,4).getContext(api),buffer=peer.createBuffer();
            gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
            if(gl.getError()!==gl.INVALID_OPERATION)throw Error('foreign ownership accepted');
            const loss=gl.getExtension('WEBGL_lose_context');loss.loseContext();
            const order=[],number=name=>({valueOf(){order.push(name);return 0}});
            gl.viewport(number('x'),number('y'),number('w'),number('h'));
            gl.bindBuffer(number('target'),buffer);
            if(String(order)!=='x,y,w,h,target')throw Error('lost conversion was skipped');
            let rejected=false;
            try{gl.bindBuffer(number('bad'),{})}catch(error){rejected=error instanceof TypeError}
            if(!rejected||order.at(-1)!=='bad')throw Error('lost interface brand conversion');
            if(gl.getError()!==gl.CONTEXT_LOST_WEBGL||gl.getError()!==0)
                throw Error('IDL exceptions modified context loss');
            peer.bindBuffer(peer.ARRAY_BUFFER,buffer);
            peer.bufferData(peer.ARRAY_BUFFER,new Uint8Array([1,2,3,4]),peer.STATIC_DRAW);
            if(peer.getBufferParameter(peer.ARRAY_BUFFER,peer.BUFFER_SIZE)!==4||peer.getError()!==0)
                throw Error('shared method plan contaminated the peer');
        }
    "#,
    );
}

#[test]
fn webgl_argument_plan_retains_texture_effective_overloads_and_buffer_size_conversion() {
    both(
        r#"
        const gl=new OffscreenCanvas(4,4).getContext('webgl');
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        const rgba=new Uint8Array([31,63,95,255]);
        const order=[],number=(name,value)=>({valueOf(){order.push(name);return value}});
        gl.texImage2D(number('target',gl.TEXTURE_2D),number('level',0),number('internal',gl.RGBA),
            number('width',1),number('height',1),number('border',0),number('format',gl.RGBA),
            number('type',gl.UNSIGNED_BYTE),rgba,{valueOf(){throw Error('surplus texture argument')}});
        if(String(order)!=='target,level,internal,width,height,border,format,type'||gl.getError()!==0)
            throw Error('nine-slot texture overload');
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if(String(pixel)!==String(rgba))throw Error('texture bytes changed');
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        order.length=0;
        gl.bufferData(number('target',gl.ARRAY_BUFFER),number('size',17.9),number('usage',gl.STATIC_DRAW));
        if(String(order)!=='target,size,usage'||gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==17)
            throw Error('numeric buffer overload');
        const bytes=new Uint8Array([1,2,3]);
        Object.defineProperty(bytes,Symbol.iterator,{value(){throw Error('buffer used sequence overload')}});
        gl.bufferData(gl.ARRAY_BUFFER,bytes,gl.STATIC_DRAW);
        if(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==3||gl.getError()!==0)
            throw Error('genuine buffer overload changed');
    "#,
    );
}
