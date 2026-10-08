// Genuine typed-array union branches and observable IDL conversion order.
// Reused by Window, Worker and an unmodified Chrome reference run.
function testNumericUnions(makeCanvas) {
    let assertions=0;
    const assert=(value,label)=>{assertions++;if(!value)throw Error(label)};
    const rejects=(fn,label)=>{
        let error;try {fn()} catch(e) {error=e}
        assert(error instanceof TypeError,label);
    };
    const poison=value=>{
        for(const name of ['buffer','byteOffset','byteLength','length',Symbol.toStringTag,Symbol.iterator])
            Object.defineProperty(value,name,{configurable:true,get(){throw Error('author property read')}});
        return value;
    };
    for(const api of ['webgl','webgl2']) {
        const gl=makeCanvas().getContext(api);
        assert(gl,api+' native context');
        const version=api==='webgl2'?'#version 300 es\n':'';
        const p=gl.createProgram();
        const fragment=version+'precision mediump float;uniform vec4 color;uniform ivec4 counts;'+
            (api==='webgl2'?'out vec4 result;':'')+'void main(){'+
            (api==='webgl2'?'result':'gl_FragColor')+'=color+vec4(counts)*0.001;}';
        const vertex=version+'uniform mat4 transform;void main(){gl_Position=transform*vec4(0.,0.,0.,1.);}';
        for(const [kind,source] of [[gl.VERTEX_SHADER,vertex],[gl.FRAGMENT_SHADER,fragment]]) {
            const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
            assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS),gl.getShaderInfoLog(shader));
            gl.attachShader(p,shader);
        }
        gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));
        gl.useProgram(p);
        const color=gl.getUniformLocation(p,'color'),counts=gl.getUniformLocation(p,'counts');
        const matrix=gl.getUniformLocation(p,'transform');
        assert(color && counts && matrix,'active uniforms');
        const float=poison(new Float32Array([.25,.5,.75,1]));
        const integer=poison(new Int32Array([1,-2,3,-4]));
        gl.uniform4fv(color,float);gl.uniform4iv(counts,integer);
        assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','typed floats ignore author properties');
        assert(String(gl.getUniform(p,counts))==='1,-2,3,-4','typed integers ignore author properties');
        gl.vertexAttrib4fv(0,float);
        assert(String(gl.getVertexAttrib(0,gl.CURRENT_VERTEX_ATTRIB))==='0.25,0.5,0.75,1','attribute typed-array branch');
        const identity=poison(new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]));
        gl.uniformMatrix4fv(matrix,false,identity);
        assert(String(gl.getUniform(p,matrix))==='1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1','matrix typed-array branch');
        const originalIterator=Float32Array.prototype[Symbol.iterator];
        try {
            Float32Array.prototype[Symbol.iterator]=()=>{throw Error('prototype iterator read')};
            gl.uniform4fv(color,new Float32Array([1,2,3,4]));
            assert(String(gl.getUniform(p,color))==='1,2,3,4','prototype iterator cannot redirect a matching union');
        } finally {Float32Array.prototype[Symbol.iterator]=originalIterator;}
        class Floats extends Float32Array {}
        const subclass=poison(new Floats([1,2,3,4]));
        gl.uniform4fv(color,subclass);
        assert(String(gl.getUniform(p,color))==='1,2,3,4','subclass retains intrinsic brand');

        // Other typed-array kinds and proxies use sequence conversion. A fake
        // toStringTag must not select the native buffer-view branch.
        let yielded=0;
        const sequence={get [Symbol.toStringTag](){throw Error('fake brand read')},
            *[Symbol.iterator](){for(const value of [.1,.2,.3,.4]){yielded++;yield value}}};
        gl.uniform4fv(color,sequence);
        assert(yielded===4 && String(gl.getUniform(p,color))===String(new Float32Array([.1,.2,.3,.4])),'fake brand uses iterable sequence');
        const other=new Float64Array(4);
        other[Symbol.iterator]=function*(){yield 4;yield 3;yield 2;yield 1};
        gl.uniform4fv(color,other);
        assert(String(gl.getUniform(p,color))==='4,3,2,1','different typed-array brand uses author iterator');
        const proxy=new Proxy(new Float32Array(4),{get(target,key){
            if(key===Symbol.iterator)return function*(){yield 7;yield 8;yield 9;yield 10};
            throw Error('unexpected proxy property');
        }});
        gl.uniform4fv(color,proxy);
        assert(String(gl.getUniform(p,color))==='7,8,9,10','proxy does not inherit typed-array internal slots');

        const rab=new ArrayBuffer(16,{maxByteLength:32});
        const resizable=new Float32Array(rab);
        rejects(()=>gl.uniform4fv(color,resizable),'resizable numeric view is not allowed');
        rab.resize(0);
        rejects(()=>gl.uniform4fv(null,resizable),'out-of-bounds resizable view still rejects before null location');
        const otherResizable=new Float64Array(new ArrayBuffer(32,{maxByteLength:64}));
        otherResizable.set([.5,.25,.125,1]);
        gl.uniform4fv(color,otherResizable);
        assert(String(gl.getUniform(p,color))==='0.5,0.25,0.125,1','nonmatching resizable kind is still an ordinary sequence');
        if(typeof SharedArrayBuffer==='function') {
            const shared=new Float32Array(new SharedArrayBuffer(16));shared.set([.5,.5,.5,1]);
            gl.uniform4fv(color,poison(shared));
            assert(String(gl.getUniform(p,color))==='0.5,0.5,0.5,1','fixed shared view is allowed');
            const growable=new Float32Array(new SharedArrayBuffer(16,{maxByteLength:32}));
            rejects(()=>gl.uniform4fv(null,growable),'growable shared view is not allowed');
        }
        assert(gl.getError()===0,'successful typed uploads and IDL exceptions do not queue errors');

        const before=String(gl.getUniform(p,color));
        const detached=new Float32Array(4);
        structuredClone(detached.buffer,{transfer:[detached.buffer]});
        for(const fn of [()=>gl.uniform4fv(color,detached),()=>gl.uniform4fv(null,detached),
            ()=>gl.vertexAttrib4fv(0,detached),()=>gl.uniformMatrix4fv(matrix,false,detached)]) {
            fn();assert(gl.getError()===gl.INVALID_VALUE,'detached view queues INVALID_VALUE, not TypeError');
            assert(gl.getError()===0,'error is consumed once');
        }
        assert(String(gl.getUniform(p,color))===before,'invalid upload leaves prior value intact');
        if(api==='webgl2') {
            gl.vertexAttribI4iv(1,poison(new Int32Array([1,-2,3,-4])));
            assert(String(gl.getVertexAttrib(1,gl.CURRENT_VERTEX_ATTRIB))==='1,-2,3,-4','integer attribute union');
            gl.vertexAttribI4uiv(1,poison(new Uint32Array([1,2,3,4294967295])));
            assert(String(gl.getVertexAttrib(1,gl.CURRENT_VERTEX_ATTRIB))==='1,2,3,4294967295','unsigned attribute union retains all bits');
            gl.clearBufferfv(gl.COLOR,0,poison(new Float32Array([1,0,0,1])));
            const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
            assert(String(pixels)==='255,0,0,255','clear-buffer numeric union reaches real pixels');
            const values=new Float32Array([1,2,3,4]);let conversions=0;
            gl.uniform4fv(color,values,{valueOf(){conversions++;values[0]=8;return 0}},0);
            assert(conversions===1 && String(gl.getUniform(p,color))==='1,2,3,4','typed values snapshot before offset conversion');
            const late=new Float32Array([5,6,7,8]);
            gl.uniform4fv(color,late,{valueOf(){structuredClone(late.buffer,{transfer:[late.buffer]});return 0}},0);
            assert(gl.getError()===0 && String(gl.getUniform(p,color))==='5,6,7,8','later detachment cannot change the owned conversion snapshot');
        }
        const loss=gl.getExtension('WEBGL_lose_context');assert(loss,'loss extension');loss.loseContext();
        gl.uniform4fv(null,detached); // Valid IDL conversion, no native work after loss.
        rejects(()=>gl.uniform4fv(null,new Float32Array(new ArrayBuffer(16,{maxByteLength:32}))),'lost context still rejects resizable view');
        let converted=0;gl.uniform4fv(null,[{valueOf(){converted++;return 1}},0,0,1]);
        assert(converted===1,'lost context still converts sequences');
        assert(gl.getError()===gl.CONTEXT_LOST_WEBGL && gl.getError()===0,'loss suppresses detached-buffer validation errors');
    }
    return {assertions};
}
