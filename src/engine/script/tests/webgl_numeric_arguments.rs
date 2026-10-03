use super::webgl_instancing::check;

#[test]
fn webgl_integer_arguments_wrap_before_native_validation() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        gl.viewport(2**32+1, -(2**32)+2, 2**32+8, 2**32+4);
        assert(String(gl.getParameter(gl.VIEWPORT))==='1,2,8,4','signed long wrapping');
        gl.viewport(NaN,Infinity,8.9,4.9);
        assert(String(gl.getParameter(gl.VIEWPORT))==='0,0,8,4','nonfinite and fractional long');
        gl.enable(gl.BLEND+2**32);
        assert(gl.isEnabled(gl.BLEND+2**32),'unsigned enum wrapping');
        gl.stencilMask(-1);
        assert(gl.getParameter(gl.STENCIL_WRITEMASK)===0xffffffff,'unsigned mask '+gl.getParameter(gl.STENCIL_WRITEMASK));
        gl.scissor(-1.9,2.9,8.9,4.9);
        assert(String(gl.getParameter(gl.SCISSOR_BOX))==='-1,2,8,4','truncate towards zero');
        assert(gl.getError()===0,'converted commands have no GL error');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_scalar_conversion_order_and_extra_arguments_follow_idl() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const order=[];
        const number=(name,value)=>({valueOf(){order.push(name);return value}});
        gl.viewport(number('x',0),number('y',0),number('w',8),number('h',4),
            {valueOf(){throw Error('extra argument converted')}});
        assert(String(order)==='x,y,w,h','left-to-right scalar conversion');
        order.length=0;
        try {gl.bindBuffer(number('target',gl.ARRAY_BUFFER),{});throw Error('wrong interface accepted')}
        catch(e){assert(e instanceof TypeError,'interface TypeError')}
        assert(String(order)==='target','numeric conversion precedes later interface');
        const shader=gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(shader,{toString(){order.push('source');return 'void main(){gl_Position=vec4(0.);}';}});
        assert(order.at(-1)==='source','DOMString conversion');
        for(const call of [()=>gl.clear(1n),()=>gl.viewport(Object(1n),0,8,4),
            ()=>gl.shaderSource(shader,Symbol()),()=>gl.viewport(Symbol(),0,8,4)]) {
            let threw=false;try{call()}catch(e){threw=e instanceof TypeError}
            assert(threw,'ToNumber/DOMString rejects invalid primitive');
        }
        assert(gl.getError()===0,'IDL failure does not set native error');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_buffer_size_overload_uses_long_long_conversion() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        for(const size of ['16',true,8.9,{valueOf(){return 12}}]) {
            gl.bufferData(gl.ARRAY_BUFFER,size,gl.STATIC_DRAW);
            assert(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)===Math.trunc(Number(size)),'numeric overload');
        }
        gl.bufferData(gl.ARRAY_BUFFER,NaN,gl.STATIC_DRAW);
        assert(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)===0,'NaN size becomes zero');
        gl.bufferData(gl.ARRAY_BUFFER,-1.9,gl.STATIC_DRAW);
        assert(gl.getError()===gl.INVALID_VALUE,'negative truncated size');
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([1,2,3]),gl.STATIC_DRAW);
        assert(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)===3,'buffer overload still selected');
        let threw=false;try{gl.bufferData(gl.ARRAY_BUFFER,1n,gl.STATIC_DRAW)}catch(e){threw=e instanceof TypeError}
        assert(threw,'BigInt is not numeric IDL input');
        assert(gl.getError()===0,'overload errors drained');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_lost_context_still_converts_scalar_and_extension_arguments() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const instancing=gl.getExtension('ANGLE_instanced_arrays');
        const vao=gl.getExtension('OES_vertex_array_object');
        gl.getExtension('WEBGL_lose_context').loseContext();
        const order=[];
        const number=label=>({valueOf(){order.push(label);return 0}});
        gl.viewport(number('x'),number('y'),number('w'),number('h'));
        assert(String(order)==='x,y,w,h','lost scalar conversion still ordered');
        instancing.vertexAttribDivisorANGLE(number('index'),number('divisor'));
        assert(String(order)==='x,y,w,h,index,divisor','lost extension numeric conversion');
        for(const call of [()=>gl.clear(1n),()=>instancing.vertexAttribDivisorANGLE(0,1n),
            ()=>vao.bindVertexArrayOES({}),()=>vao.deleteVertexArrayOES({}),()=>vao.isVertexArrayOES({})]) {
            let threw=false;try{call()}catch(e){threw=e instanceof TypeError}
            assert(threw,'lost context does not bypass IDL');
        }
        vao.bindVertexArrayOES(undefined);vao.deleteVertexArrayOES(undefined);
        assert(vao.isVertexArrayOES(undefined)===false,'nullable extension interface');
        assert(gl.getError()===gl.CONTEXT_LOST_WEBGL && gl.getError()===0,'IDL failure leaves loss error intact');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_float_parameters_round_to_single_precision_without_truncation() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const value=0.123456789;
        gl.clearColor(value,value,value,value);
        assert(gl.getParameter(gl.COLOR_CLEAR_VALUE).every(v=>v===Math.fround(value)),'float rounding');
        gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
        gl.texParameterf(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
        assert(gl.getTexParameter(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER)===gl.NEAREST,'native float texture parameter');
        gl.texParameterf(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,1.5);
        assert(gl.getError()===gl.INVALID_ENUM,'invalid float enum rejected by driver');
        assert(gl.getError()===0,'errors drained');
        document.querySelector('output').textContent='pass';
    "#,
    );
}
