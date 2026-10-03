use super::webgl_instancing::{SETUP, check};

#[test]
fn webgl_derivative_directive_may_follow_non_preprocessor_code() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        gl.getExtension('OES_standard_derivatives');
        const shader=gl.createShader(gl.FRAGMENT_SHADER);
        const source='precision mediump float;\nvarying vec2 texCoord;\nvoid main() {\n'+
            '#extension GL_OES_standard_derivatives : enable\n'+
            'gl_FragColor=vec4(dFdx(texCoord.x),dFdy(texCoord.y),fwidth(texCoord.x),1.);\n}';
        gl.shaderSource(shader,source);gl.compileShader(shader);
        if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(shader));
        if(gl.getShaderSource(shader)!==source)throw Error('shader source changed');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_uint_indices_require_extension_and_validate_full_32_bit_values() {
    check(&format!(
        r#"{SETUP}
        const index=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,index);
        gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint32Array([0,1,2,3]),gl.STATIC_DRAW);
        gl.drawElements(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,0);
        assert(gl.getError()===gl.INVALID_ENUM,'uint disabled');
        const uint=gl.getExtension('OES_element_index_uint');assert(uint,'uint supported');
        assert(uint===gl.getExtension('oes_ELEMENT_index_uint'),'cached case insensitive');
        assert(Object.prototype.toString.call(uint)==='[object OES_element_index_uint]','extension brand');
        gl.drawElements(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,0);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255','ordinary uint draw');
        ext.drawElementsInstancedANGLE(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,0,2);
        assert(gl.getError()===0 && pixel(6,1)==='0,255,0,255','instanced uint draw');
        gl.clear(gl.COLOR_BUFFER_BIT);
        gl.bufferSubData(gl.ELEMENT_ARRAY_BUFFER,0,new Uint32Array([65536,1,2,3]));
        gl.drawElements(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,0);
        assert(gl.getError()===gl.INVALID_OPERATION,'32 bit indices not truncated');
        ext.drawElementsInstancedANGLE(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,0,2);
        assert(gl.getError()===gl.INVALID_OPERATION,'instanced 32 bit indices not truncated');
        gl.drawElements(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_INT,2);
        assert(gl.getError()===gl.INVALID_OPERATION,'uint alignment');
        gl.drawElements(gl.TRIANGLE_STRIP,5,gl.UNSIGNED_INT,0);
        assert(gl.getError()===gl.INVALID_OPERATION,'uint byte range');
        assert(pixel(1,1)==='0,0,0,0','invalid uint draws do not paint');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_standard_derivatives_use_native_shader_compiler_and_real_pixel_gradients() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(value,label)=>{if(!value)throw Error(label)};
        gl.getParameter(0x8b8b);assert(gl.getError()===gl.INVALID_ENUM,'hint unavailable before extension');
        gl.hint(0x8b8b,gl.NICEST);assert(gl.getError()===gl.INVALID_ENUM,'hint target unavailable before extension');
        const ext=gl.getExtension('OES_standard_derivatives');assert(ext,'native derivative capability');
        assert(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES===0x8b8b,'hint token');
        gl.hint(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES,gl.NICEST);
        assert(gl.getParameter(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES)===gl.NICEST,'native hint');
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        const program=gl.createProgram();
        gl.attachShader(program,compile(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
        gl.attachShader(program,compile(gl.FRAGMENT_SHADER,
            '#extension GL_OES_standard_derivatives : require\nprecision mediump float;void main(){gl_FragColor=vec4(dFdx(gl_FragCoord.x),dFdy(gl_FragCoord.y),fwidth(gl_FragCoord.x),1.);}'));
        gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
        assert(gl.getProgramParameter(program,gl.LINK_STATUS),'native derivative link');gl.useProgram(program);
        const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        const pixels=new Uint8Array(8*4*4);gl.readPixels(0,0,8,4,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(gl.getError()===0,'derivative draw error');
        assert(pixels.every(value=>value===255),'native derivative gradients produce white quad');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_standard_derivative_hint_bad_mode_preserves_previous_state() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl'), ext=gl.getExtension('OES_standard_derivatives');
        if(!ext)throw Error('derivative extension absent');
        for(const mode of [gl.FASTEST,gl.NICEST,gl.DONT_CARE]) {
            gl.hint(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES,mode);
            if(gl.getError()!==0 || gl.getParameter(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES)!==mode)throw Error('valid hint rejected');
            gl.hint(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES,0xdead);
            if(gl.getError()!==gl.INVALID_ENUM || gl.getParameter(ext.FRAGMENT_SHADER_DERIVATIVE_HINT_OES)!==mode)throw Error('invalid hint changed state');
        }
        document.querySelector('output').textContent='pass';
    "#,
    );
}
#[test]
fn webgl_extension_lookup_uses_required_domstring_conversion_before_loss_handling() {
    super::webgl_instancing::check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        let reads=0;
        const ext=gl.getExtension({toString(){reads++;return 'ANGLE_instanced_arrays'}});
        assert(ext && reads===1 && ext===gl.getExtension('angle_INSTANCED_arrays'),'one DOMString conversion');
        for(const call of [()=>gl.getExtension(),()=>gl.getExtension(Symbol('name')),
            ()=>WebGLRenderingContext.prototype.getExtension.call({},'ANGLE_instanced_arrays')]) {
            let threw=false;try{call()}catch(e){threw=e instanceof TypeError}assert(threw,'required IDL conversion');
        }
        assert(gl.getExtension(null)===null && gl.getExtension(undefined)===null,'explicit null and undefined strings');
        const loss=gl.getExtension('WEBGL_lose_context');loss.loseContext();
        const absent=gl.getExtension({toString(){reads++;return 'ANGLE_instanced_arrays'}});
        assert(absent===null && reads===2,'lost context still converts names');
        let threw=false;try{gl.getExtension(Symbol('lost'))}catch(e){threw=e instanceof TypeError}
        assert(threw,'lost context still rejects Symbols');
        document.querySelector('output').textContent='pass';
    "#,
    );
}
