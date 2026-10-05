// Shared Window/OffscreenCanvas contract, using real shaders and FBO readback.
function testIndexedBlend(createCanvas) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const canvas=createCanvas();canvas.width=4;canvas.height=4;
    const gl=canvas.getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
    assert(gl,'WebGL2 context');
    const ext=gl.getExtension('OES_draw_buffers_indexed');
    assert(ext,'real native indexed extension');
    assert(ext===gl.getExtension('oes_DRAW_BUFFERS_indexed'),'case-insensitive identity');
    assert(!('OES_draw_buffers_indexed' in globalThis),'no interface object');
    assert(!('isEnablediOES' in ext),'native-only getter is not exposed');
    const brand=Object.prototype.toString.call(ext);
    for(const [name,arity] of [['enableiOES',2],['disableiOES',2],['blendEquationiOES',2],
        ['blendEquationSeparateiOES',3],['blendFunciOES',3],['blendFuncSeparateiOES',5],['colorMaskiOES',5]]) {
        const descriptor=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(ext),name);
        assert(descriptor.enumerable&&descriptor.writable&&descriptor.configurable,'IDL descriptor '+name);
        assert(ext[name].length===arity&&ext[name].name===name,'IDL signature '+name);
        assert(!(name in gl),'extension-only method '+name);
        let error;try{ext[name].call({});}catch(e){error=e;}
        assert(error instanceof TypeError,'receiver '+name);
        error=null;try{ext[name]();}catch(e){error=e;}
        assert(error instanceof TypeError,'arity '+name);
    }
    const trace=[],value=(name,n)=>({valueOf(){trace.push(name);return n;}});
    ext.blendFuncSeparateiOES(value('index',0),value('sr',1),value('dr',0),value('sa',1),value('da',0));
    assert(trace.join() === 'index,sr,dr,sa,da','ordered numeric conversion');
    let typeError=false;try{ext.blendFunciOES(0,1n,0);}catch(e){typeError=e instanceof TypeError;}
    assert(typeError,'BigInt GLenum rejection');
    ext.colorMaskiOES(1,{},'',1,0);
    const returned=gl.getIndexedParameter(gl.COLOR_WRITEMASK,1);
    assert(Array.isArray(returned)&&returned.join()==='true,false,true,false','boolean sequence');
    returned[0]=false;
    assert(gl.getIndexedParameter(gl.COLOR_WRITEMASK,1)[0]===true,'owned return sequence');
    const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);
    for(let index=0;index<2;index++) {
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,4,4,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0+index,gl.TEXTURE_2D,texture,0);
    }
    gl.drawBuffers([gl.COLOR_ATTACHMENT0,gl.COLOR_ATTACHMENT1]);
    assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'complete MRT');
    const shader=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
    const program=gl.createProgram();
    gl.attachShader(program,shader(gl.VERTEX_SHADER,'#version 300 es\nvoid main(){gl_Position=vec4(float(gl_VertexID==1?3:-1),float(gl_VertexID==2?3:-1),0,1);}'));
    gl.attachShader(program,shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;layout(location=0) out vec4 a;layout(location=1) out vec4 b;void main(){a=vec4(1,0,0,0.5);b=vec4(0,1,0,0.5);}'));
    gl.linkProgram(program);assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
    const pixel=index=>{gl.readBuffer(gl.COLOR_ATTACHMENT0+index);const bytes=new Uint8Array(4);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,bytes);return Array.from(bytes);};
    gl.colorMask(true,true,true,true);gl.clearColor(0,0,1,1);gl.clear(gl.COLOR_BUFFER_BIT);
    ext.enableiOES(gl.BLEND,0);ext.disableiOES(gl.BLEND,1);
    ext.blendFuncSeparateiOES(0,gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ZERO);
    ext.colorMaskiOES(1,false,true,false,false);
    gl.drawArrays(gl.TRIANGLES,0,3);
    const first=pixel(0),second=pixel(1);
    assert(Math.abs(first[0]-128)<=1&&first[1]===0&&Math.abs(first[2]-128)<=1&&Math.abs(first[3]-128)<=1,'independent blend pixels '+first);
    assert(second.join()==='0,255,255,255','independent mask pixels '+second);
    assert(gl.getError()===gl.NO_ERROR,'MRT operations succeeded');
    ext.enableiOES(gl.BLEND,1);
    ext.blendFunciOES(0,gl.CONSTANT_COLOR,gl.ONE);
    ext.blendFunciOES(1,gl.CONSTANT_ALPHA,gl.ONE);
    gl.drawArrays(gl.TRIANGLES,0,3);
    assert(gl.getError()===gl.INVALID_OPERATION,'cross-buffer constant-color/alpha rule');
    assert(pixel(0).join()===first.join()&&pixel(1).join()===second.join(),'invalid draw is atomic');
    gl.blendFunc(gl.ONE,gl.ZERO);gl.blendEquation(gl.FUNC_ADD);gl.colorMask(true,true,true,true);gl.disable(gl.BLEND);
    gl.drawArrays(gl.TRIANGLES,0,3);
    assert(pixel(0)[0]===255&&pixel(0)[2]===0&&pixel(1)[1]===255&&pixel(1)[2]===0,'global broadcasts affect both outputs');
    assert(gl.getIndexedParameter(gl.BLEND_SRC_RGB,1)===gl.ONE,'global factor query');
    ext.blendEquationiOES(gl.getParameter(gl.MAX_DRAW_BUFFERS),gl.FUNC_ADD);
    assert(gl.getError()===gl.INVALID_VALUE,'index limit');
    ext.enableiOES(gl.SCISSOR_TEST,0);assert(gl.getError()===gl.INVALID_ENUM,'closed capability');
    assert(gl.getError()===gl.NO_ERROR,'no residual error');
    return {first,second,brand,passed:true};
}
