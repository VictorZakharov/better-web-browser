use super::webgl_instancing::check;

pub(in crate::engine::script) const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const error=(expected,label)=>{const actual=gl.getError();assert(actual===expected,label+' '+actual)};
    const ext=gl.getExtension('WEBGL_draw_buffers');assert(ext,'native MRT support');
    const texture=()=>{
        const t=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,t);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);return t;
    };
    const pixel=()=>{const p=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);error(0,'read');return [...p].join();};
    const inspect=t=>{
        const f=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,f);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,t,0);
        const result=pixel();gl.deleteFramebuffer(f);return result;
    };
    const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
    const textures=[texture(),texture(),texture(),texture()];
    textures.forEach((t,i)=>gl.framebufferTexture2D(gl.FRAMEBUFFER,ext.COLOR_ATTACHMENT0_WEBGL+i,gl.TEXTURE_2D,t,0));
    assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'four guaranteed color targets');
    ext.drawBuffersWEBGL(textures.map((_,i)=>ext.COLOR_ATTACHMENT0_WEBGL+i));error(0,'route MRT');
    const shader=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
    const program=gl.createProgram();
    gl.attachShader(program,shader(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
    gl.attachShader(program,shader(gl.FRAGMENT_SHADER,
        '#extension GL_EXT_draw_buffers : require\nprecision mediump float;void main(){gl_FragData[0]=vec4(1,0,0,1);gl_FragData[1]=vec4(0,1,0,1);gl_FragData[2]=vec4(0,0,1,1);gl_FragData[3]=vec4(1);}'));
    gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
    assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
    gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
    gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);error(0,'setup');
"#;

#[test]
fn webgl_multiple_render_targets_write_distinct_native_images_and_reflect_identity() {
    check(&format!(
        r#"{SETUP}
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'MRT draw');
        textures.forEach((t,i)=>assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,
            ext.COLOR_ATTACHMENT0_WEBGL+i,gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME)===t,'logical attachment '+i));
        const colors=['255,0,0,255','0,255,0,255','0,0,255,255','255,255,255,255'];
        textures.forEach((t,i)=>assert(inspect(t)===colors[i],'native MRT pixel '+i));
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_draw_buffer_routing_is_framebuffer_local_and_failed_routes_are_atomic() {
    check(&format!(
        r#"{SETUP}
        assert(gl.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL)>=4,'at least four outputs');
        assert(gl.getParameter(ext.MAX_COLOR_ATTACHMENTS_WEBGL)>=gl.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL),'attachment limit');
        ext.drawBuffersWEBGL([gl.NONE,ext.COLOR_ATTACHMENT1_WEBGL]);error(0,'sparse routing');
        assert(gl.getParameter(ext.DRAW_BUFFER0_WEBGL)===gl.NONE,'disabled slot');
        assert(gl.getParameter(ext.DRAW_BUFFER1_WEBGL)===ext.COLOR_ATTACHMENT1_WEBGL,'enabled slot');
        ext.drawBuffersWEBGL([ext.COLOR_ATTACHMENT1_WEBGL]);error(gl.INVALID_OPERATION,'wrong index');
        assert(gl.getParameter(ext.DRAW_BUFFER0_WEBGL)===gl.NONE,'failure preserves route');
        ext.drawBuffersWEBGL(new Array(gl.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL)+1).fill(gl.NONE));
        error(gl.INVALID_VALUE,'oversized route');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        assert(gl.getParameter(ext.DRAW_BUFFER0_WEBGL)===gl.BACK,'private surface reflected as BACK');
        ext.drawBuffersWEBGL([ext.COLOR_ATTACHMENT0_WEBGL]);error(gl.INVALID_OPERATION,'hidden attachment not exposed');
        ext.drawBuffersWEBGL([]);error(gl.INVALID_OPERATION,'default requires one slot');
        ext.drawBuffersWEBGL([gl.NONE]);error(0,'disable default');
        gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        assert(gl.getParameter(ext.DRAW_BUFFER1_WEBGL)===ext.COLOR_ATTACHMENT1_WEBGL,'FBO route restored');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        assert(gl.getParameter(ext.DRAW_BUFFER0_WEBGL)===gl.NONE,'default route restored');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_duplicate_color_images_are_unsupported_and_deletion_detaches_every_slot() {
    check(&format!(
        r#"{SETUP}
        gl.framebufferTexture2D(gl.FRAMEBUFFER,ext.COLOR_ATTACHMENT1_WEBGL,gl.TEXTURE_2D,textures[0],0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_UNSUPPORTED,'same image twice');
        gl.clear(gl.COLOR_BUFFER_BIT);error(gl.INVALID_FRAMEBUFFER_OPERATION,'no duplicate writes');
        gl.framebufferTexture2D(gl.FRAMEBUFFER,ext.COLOR_ATTACHMENT1_WEBGL,gl.TEXTURE_2D,textures[1],0);
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'detachment recovery');
        gl.deleteTexture(textures[2]);error(0,'delete attached color');
        assert(gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,ext.COLOR_ATTACHMENT2_WEBGL,
            gl.FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE)===gl.NONE,'deleted nonzero color detached');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_draw_buffers_idl_brands_iterators_and_context_gating_are_enforced() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(value,label)=>{if(!value)throw Error(label)};
        const throws=f=>{let thrown=false;try{f()}catch(e){thrown=e instanceof TypeError}assert(thrown,'TypeError')};
        assert(gl.getParameter(0x8824)===null && gl.getError()===gl.INVALID_ENUM,'query gated');
        gl.framebufferTexture2D(gl.FRAMEBUFFER,0x8ce1,gl.TEXTURE_2D,null,0);
        assert(gl.getError()===gl.INVALID_OPERATION,'default surface protected');
        const ext=gl.getExtension('WEBGL_draw_buffers');assert(ext,'extension');
        assert(ext===gl.getExtension('webgl_DRAW_buffers'),'case insensitive cached object');
        assert(typeof WEBGL_draw_buffers==='undefined','legacy no interface object');
        assert(Object.prototype.toString.call(ext)==='[object WEBGL_draw_buffers]','extension brand');
        assert(ext.drawBuffersWEBGL.length===1,'IDL arity');
        for(let i=0;i<16;i++)assert(ext['COLOR_ATTACHMENT'+i+'_WEBGL']===0x8ce0+i &&
            ext['DRAW_BUFFER'+i+'_WEBGL']===0x8825+i,'constant '+i);
        throws(()=>ext.drawBuffersWEBGL());throws(()=>ext.drawBuffersWEBGL.call({},[gl.BACK]));
        throws(()=>ext.drawBuffersWEBGL(null));throws(()=>ext.drawBuffersWEBGL({0:gl.BACK,length:1}));
        throws(()=>ext.drawBuffersWEBGL([1n]));
        assert(ext.drawBuffersWEBGL(new Set([gl.BACK]))===undefined,'iterable void return');
        assert(gl.getError()===0,'IDL failures do not enqueue native errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_mrt_draw_requires_outputs_for_every_enabled_target_unless_color_writes_are_disabled() {
    check(&format!(
        r#"{SETUP}
        const single=gl.createProgram();
        gl.attachShader(single,shader(gl.VERTEX_SHADER,'attribute vec2 p;void main(){{gl_Position=vec4(p,0,1);}}'));
        gl.attachShader(single,shader(gl.FRAGMENT_SHADER,'precision mediump float;void main(){{gl_FragColor=vec4(1);}}'));
        gl.bindAttribLocation(single,0,'p');gl.linkProgram(single);gl.useProgram(single);
        assert(gl.getProgramParameter(single,gl.LINK_STATUS),'single target program');
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(gl.INVALID_OPERATION,'missing fragment outputs');
        gl.colorMask(false,false,false,false);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'no color writes permits missing outputs');
        gl.colorMask(true,true,true,true);
        ext.drawBuffersWEBGL([ext.COLOR_ATTACHMENT0_WEBGL]);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'route matches output');
        assert(inspect(textures[0])==='255,255,255,255','single output painted');
        for(let i=1;i<4;i++)assert(inspect(textures[i])==='0,0,0,0','no broadcast '+i);
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_mrt_maximum_is_frozen_by_compilation_and_extension_is_not_inherited_by_peer_context() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(value,label)=>{if(!value)throw Error(label)};
        const compile=source=>{const s=gl.createShader(gl.FRAGMENT_SHADER);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        const before=compile('precision mediump float;void main(){gl_FragColor=vec4(float(gl_MaxDrawBuffers)/8.);}');
        const ext=gl.getExtension('WEBGL_draw_buffers');assert(ext,'native support');
        const after=compile('precision mediump float;void main(){gl_FragColor=vec4(float(gl_MaxDrawBuffers)/8.);}');
        const vertex=gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(vertex,'attribute vec2 p;void main(){gl_Position=vec4(p,0,1);}');gl.compileShader(vertex);
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        const draw=s=>{const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,s);
            gl.bindAttribLocation(p,0,'p');gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));
            gl.useProgram(p);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
            const bytes=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
            assert(gl.getError()===0,'draw frozen limit');return bytes[0];};
        assert(Math.abs(draw(before)-32)<=1,'old compile preserves one output');
        assert(Math.abs(draw(after)-Math.min(255,gl.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL)*255/8))<=1,'new compile sees enabled limit');
        const peer=document.createElement('canvas').getContext('webgl');
        peer.bindFramebuffer(peer.FRAMEBUFFER,peer.createFramebuffer());
        peer.framebufferTexture2D(peer.FRAMEBUFFER,ext.COLOR_ATTACHMENT1_WEBGL,peer.TEXTURE_2D,null,0);
        assert(peer.getError()===peer.INVALID_ENUM,'attachment enum gated in peer');
        assert(peer.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL)===null && peer.getError()===peer.INVALID_ENUM,'limit gated in peer');
        const s=peer.createShader(peer.FRAGMENT_SHADER);
        peer.shaderSource(s,'#extension GL_EXT_draw_buffers : require\nprecision mediump float;void main(){gl_FragData[1]=vec4(1);}');
        peer.compileShader(s);assert(!peer.getShaderParameter(s,peer.COMPILE_STATUS),'shader directive gated in peer');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_mrt_restoration_resets_routes_and_invalidates_old_extension_receiver() {
    super::webgl_lifecycle::run(
        r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const ext=gl.getExtension('WEBGL_draw_buffers'),loss=gl.getExtension('WEBGL_lose_context');
        ext.drawBuffersWEBGL([gl.NONE]);assert(gl.getParameter(ext.DRAW_BUFFER0_WEBGL)===gl.NONE,'old route');
        canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();setTimeout(()=>loss.restoreContext(),0)});
        canvas.addEventListener('webglcontextrestored',()=>{
            assert(gl.getParameter(ext.MAX_DRAW_BUFFERS_WEBGL)===null && gl.getError()===gl.INVALID_ENUM,'restored query disabled');
            const current=gl.getExtension('WEBGL_draw_buffers');assert(current && current!==ext,'new extension');
            assert(gl.getParameter(current.DRAW_BUFFER0_WEBGL)===gl.BACK,'fresh default route');
            let converted=0;ext.drawBuffersWEBGL([{valueOf(){converted++;return gl.NONE}}]);
            assert(converted===1 && gl.getParameter(current.DRAW_BUFFER0_WEBGL)===gl.BACK,'stale receiver converts but cannot mutate');
            assert(gl.getError()===0,'stale operation is harmless');canvas.dataset.result='pass';
        });loss.loseContext();
    "#,
    );
}
