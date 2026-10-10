//! Larger float uniforms bypass JSON without changing IDL snapshot/observation semantics.
use super::webgl_owned_copy_tests::both;
use super::webgl2_bindings_tests::{check, document};

const PROGRAM: &str = r#"
    const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false});
    const assert=(ok,label)=>{if(!ok)throw Error(label)};
    const program=gl.createProgram();
    for(const [kind,source] of [
        [gl.VERTEX_SHADER,'#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);gl_PointSize=1.;}'],
        [gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;uniform vec4 colors[64];uniform int selected;out vec4 color;void main(){color=colors[selected];}']
    ]) {
        const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
        assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS),gl.getShaderInfoLog(shader));gl.attachShader(program,shader);
    }
    gl.linkProgram(program);assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
    const location=gl.getUniformLocation(program,'colors[0]');
    const last=gl.getUniformLocation(program,'colors[63]');
    const expect=()=>{
        assert(String(gl.getUniform(program,location))==='0.25,0.5,0.75,1','first uniform snapshot');
        assert(String(gl.getUniform(program,last))==='0.25,0.5,0.75,1','last uniform snapshot');
        assert(gl.getError()===0,'native GL validation');
    };
    const make=()=>{const data=new Float32Array(264);for(let i=4;i<260;i+=4)data.set([.25,.5,.75,1],i);return data};
"#;

#[test]
fn large_uniform_fixed_range_is_owned_before_mutation_or_transfer_in_both_realms() {
    both(&format!(
        r#"{PROGRAM}
        const data=make(),view=new Float32Array(data.buffer,16,256);
        Object.defineProperties(view,{{length:{{get(){{throw Error('author length')}}}},
            byteOffset:{{get(){{throw Error('author offset')}}}},buffer:{{get(){{throw Error('author buffer')}}}}}});
        gl.uniform4fv(location,view);data.fill(0);data.buffer.transfer();expect();
    "#
    ));
}

#[test]
fn large_uniform_observable_tail_keeps_the_early_idl_snapshot() {
    both(&format!(
        r#"{PROGRAM}
        const data=make();let reads=0;
        const offset={{valueOf(){{reads++;data.fill(0);data.buffer.transfer();return 4}}}};
        gl.uniform4fv(location,data,offset,256);assert(reads===1,'offset conversion count');expect();
    "#
    ));
}

#[test]
fn large_uniform_shared_source_is_independently_copied() {
    both(&format!(
        r#"{PROGRAM}
        const data=new Float32Array(new SharedArrayBuffer(1024));
        for(let i=0;i<256;i+=4)data.set([.25,.5,.75,1],i);
        gl.uniform4fv(location,data);data.fill(0);expect();
    "#
    ));
}

#[test]
fn full_float_ranges_reuse_views_but_never_author_storage_after_the_call() {
    both(&format!(
        r#"{PROGRAM}
        for(const count of [4,256]) {{
            const backing=new Float32Array(count+8),view=new Float32Array(backing.buffer,16,count);
            for(let i=0;i<count;i+=4)view.set([.25,.5,.75,1],i);
            Object.defineProperties(view,{{buffer:{{get(){{throw Error('author buffer')}}}},
                length:{{get(){{throw Error('author length')}}}},constructor:{{get(){{throw Error('species')}}}}}});
            gl.uniform4fv(location,view);
            backing.fill(0);backing.buffer.transfer();
            assert(String(gl.getUniform(program,location))==='0.25,0.5,0.75,1','owned full range');
            assert(gl.getError()===0,'full range native validation');
            gl.uniform4fv(location,view);
            assert(gl.getError()===gl.INVALID_VALUE,'detached full range');
        }}
    "#
    ));
}

#[test]
fn large_uniform_uses_numeric_transport_without_hiding_packet_order_or_gl_errors() {
    let (mut context, host) = document();
    check(&mut context, PROGRAM);
    host.borrow_mut().host_call_profile.set_enabled(true);
    check(
        &mut context,
        r#"
        gl.clearColor(0,1,0,1);
        gl.uniform4fv(location,make(),4,256);
        gl.clear(gl.COLOR_BUFFER_BIT);expect();
        assert(String(gl.getParameter(gl.COLOR_CLEAR_VALUE))==='0,1,0,1','preceding packet retained');
        gl.uniform4fv(location,new Float32Array(65));
        assert(gl.getError()===gl.INVALID_VALUE,'invalid vector shape retains native error');
        expect();
    "#,
    );
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandValues"),
        2
    );
    assert!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandPacket")
            >= 2
    );
}
