use super::webgl_instancing::check;

const PROGRAM: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl');
    const assert=(v,s)=>{if(!v)throw Error(s)};
    const p=gl.createProgram();
    for(const [kind,source] of [[gl.VERTEX_SHADER,'void main(){gl_Position=vec4(0.);}'],
        [gl.FRAGMENT_SHADER,'precision mediump float;uniform vec4 color;uniform ivec2 counts;void main(){gl_FragColor=color+vec4(float(counts.x+counts.y));}']]) {
        const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));gl.attachShader(p,s);
    }
    gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));gl.useProgram(p);
    const color=gl.getUniformLocation(p,'color'),counts=gl.getUniformLocation(p,'counts');
"#;

#[test]
fn webgl_uniform_lists_convert_each_element_once_in_iterator_order() {
    check(&format!(
        r#"{PROGRAM}
        const order=[];
        const values={{*[Symbol.iterator](){{
            for(let i=0;i<4;i++){{order.push('yield'+i);yield {{valueOf(){{order.push('convert'+i);return i/10}}}};}}
        }}}};
        gl.uniform4fv(color,values);
        assert(String(order)==='yield0,convert0,yield1,convert1,yield2,convert2,yield3,convert3','sequence conversion order');
        let getters=0,calls=0;
        const iterable={{get [Symbol.iterator](){{getters++;return function(){{calls++;return [0,0,0,1][Symbol.iterator]()}}}}}};
        gl.uniform4fv(color,iterable);
        assert(getters===1 && calls===1,'iterator method accessed and invoked once');
        gl.uniform4fv(color,[0,.1,.2,.3]);
        const actual=gl.getUniform(p,color);
        assert(actual instanceof Float32Array && String(actual)===String(new Float32Array([0,.1,.2,.3])),'single precision list');
        gl.uniform2iv(counts,[2**32+1,-(2**32)-2]);
        assert(String(gl.getUniform(p,counts))==='1,-2','integer sequence wraps');
        gl.uniform4fv(color,new Float64Array([.1,.2,.3,.4]));
        assert(String(gl.getUniform(p,color))===String(new Float32Array([.1,.2,.3,.4])),'other iterable typed arrays use sequence overload');
        assert(gl.getError()===0,'sequence upload has no native errors');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_numeric_list_failure_closes_iterator_and_does_not_modify_uniform() {
    check(&format!(
        r#"{PROGRAM}
        gl.uniform4fv(color,[1,2,3,4]);
        let closed=false;
        const broken={{*[Symbol.iterator](){{try{{yield 0;yield Symbol();yield 9}}finally{{closed=true}}}}}};
        let threw=false;try{{gl.uniform4fv(color,broken)}}catch(e){{threw=e instanceof TypeError}}
        assert(threw && closed,'IDL exception closes iterator');
        assert(String(gl.getUniform(p,color))==='1,2,3,4','failed conversion is atomic for native state');
        for(const value of [undefined,null,{{0:1,length:4}},[1n,0,0,1]]){{
            let rejected=false;try{{gl.uniform4fv(color,value)}}catch(e){{rejected=e instanceof TypeError}}
            assert(rejected,'invalid sequence rejected');
        }}
        gl.uniform4fv(null,[.1,.2,.3,.4]);
        let rejected=false;try{{gl.uniform4fv(null,[Symbol()])}}catch(e){{rejected=e instanceof TypeError}}
        assert(rejected,'null location still converts list');
        assert(gl.getError()===0,'IDL errors do not queue GL errors');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_unrestricted_uniform_floats_survive_private_transport_and_readback() {
    check(&format!(
        r#"{PROGRAM}
        gl.uniform4f(color,NaN,Infinity,-Infinity,-0);
        const value=gl.getUniform(p,color);
        assert(Number.isNaN(value[0]),'NaN retained');
        assert(value[1]===Infinity && value[2]===-Infinity,'infinities retained');
        assert(Object.is(value[3],-0),'signed zero retained');
        gl.uniform4fv(color,new Float32Array([NaN,Infinity,-Infinity,-0]));
        const list=gl.getUniform(p,color);
        assert(Number.isNaN(list[0]) && list[1]===Infinity && list[2]===-Infinity && Object.is(list[3],-0),'typed unrestricted float list');
        gl.vertexAttrib4f(0,NaN,Infinity,-Infinity,-0);
        const attribute=gl.getVertexAttrib(0,gl.CURRENT_VERTEX_ATTRIB);
        assert(Number.isNaN(attribute[0]) && attribute[1]===Infinity && attribute[2]===-Infinity && Object.is(attribute[3],-0),'attribute unrestricted floats');
        gl.clearColor(Infinity,-Infinity,0,1);
        const clear=gl.getParameter(gl.COLOR_CLEAR_VALUE);
        assert(clear[0]===Infinity && clear[1]===-Infinity && clear[2]===0 && clear[3]===1,'unrestricted clear state matches Chrome');
        assert(gl.getError()===0,'unrestricted floats are not INVALID_VALUE');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_lost_context_still_consumes_sequences_and_validates_buffer_interfaces() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        gl.getExtension('WEBGL_lose_context').loseContext();
        let converted=0;
        gl.uniform4fv(null,[{valueOf(){converted++;return 1}},0,0,1]);
        assert(converted===1,'lost context consumes sequence');
        for(const call of [()=>gl.uniform4fv(null,[Symbol()]),()=>gl.vertexAttrib4fv(0,[1n]),
            ()=>gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,{}),
            ()=>gl.bufferSubData(gl.ARRAY_BUFFER,0,{}),
            ()=>gl.compressedTexImage2D(gl.TEXTURE_2D,0,1,1,1,0,{})]) {
            let threw=false;try{call()}catch(e){threw=e instanceof TypeError}
            assert(threw,'lost context still validates sequence/buffer interface');
        }
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,null);
        assert(gl.getError()===gl.CONTEXT_LOST_WEBGL && gl.getError()===0,'lost conversions preserve loss error');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_sequence_budget_is_bounded_and_closes_infinite_iterator() {
    check(&format!(
        r#"{PROGRAM}
        let closed=false,visited=0;
        const endless={{*[Symbol.iterator](){{try{{while(true){{visited++;yield 0}}}}finally{{closed=true}}}}}};
        let threw=false;try{{gl.uniform4fv(color,endless)}}catch(e){{threw=e instanceof RangeError}}
        assert(threw && closed && visited===8193,'bounded sequence conversion closes iterator');
        assert(gl.getError()===0,'conversion budget does not call the driver');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
