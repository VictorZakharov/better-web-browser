use super::webgl_instancing::check;

#[test]
fn webgl_fragment_depth_controls_real_native_depth_testing() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{depth:true,preserveDrawingBuffer:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const ext=gl.getExtension('EXT_frag_depth');assert(ext,'native fragment depth capability');
        assert(Object.prototype.toString.call(ext)==='[object EXT_frag_depth]','extension brand');
        assert(typeof EXT_frag_depth==='undefined','no public constructor');
        assert(ext===gl.getExtension('ext_FRAG_depth'),'cached case-insensitive extension');
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        const program=gl.createProgram();
        gl.attachShader(program,compile(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
        gl.attachShader(program,compile(gl.FRAGMENT_SHADER,
            '#extension GL_EXT_frag_depth : require\nprecision mediump float;uniform float depth;uniform vec4 color;void main(){gl_FragColor=color;gl_FragDepthEXT=depth;}'));
        gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
        assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        const depth=gl.getUniformLocation(program,'depth'),color=gl.getUniformLocation(program,'color');
        const pixel=()=>{const v=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,v);return String(v)};
        gl.enable(gl.DEPTH_TEST);gl.depthFunc(gl.LESS);gl.clearDepth(1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
        gl.uniform1f(depth,.25);gl.uniform4fv(color,[1,0,0,1]);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        assert(pixel()==='255,0,0,255','first fragment writes depth .25');
        gl.uniform1f(depth,.75);gl.uniform4fv(color,[0,1,0,1]);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        assert(pixel()==='255,0,0,255','later fragment behind stored depth rejected');
        gl.uniform1f(depth,.1);gl.uniform4fv(color,[0,0,1,1]);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        assert(pixel()==='0,0,255,255','nearer shader depth wins');
        gl.disable(gl.DEPTH_TEST);gl.uniform1f(depth,.9);gl.uniform4fv(color,[0,1,0,1]);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        assert(pixel()==='0,255,0,255','disabled depth test still bypasses fragment depth');
        assert(gl.getError()===0,'fragment-depth drawing has no native errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_fragment_depth_requires_explicit_extension_and_directive() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const shader=gl.createShader(gl.FRAGMENT_SHADER);
        const body='precision mediump float;void main(){gl_FragColor=vec4(1.);gl_FragDepthEXT=.5;}';
        const compile=source=>{gl.shaderSource(shader,source);gl.compileShader(shader);return gl.getShaderParameter(shader,gl.COMPILE_STATUS)};
        assert(!compile('#extension GL_EXT_frag_depth : require\n'+body),'disabled extension rejected');
        gl.getExtension('EXT_frag_depth');
        assert(!compile(body),'builtin disabled without shader directive');
        assert(compile('#extension GL_EXT_frag_depth : enable\n'+body),'enabled builtin compiled');
        assert(!compile('#extension GL_EXT_frag_depth : disable\n'+body),'disable directive respected');
        assert(compile('#extension GL_EXT_frag_depth : warn\n'+body),'warn directive permits builtin');
        assert(gl.getError()===0,'compiler failures do not queue API errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}
