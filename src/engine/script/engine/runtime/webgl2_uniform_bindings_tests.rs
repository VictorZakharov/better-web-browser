//! Typed reflection and rejected GPU state transitions at the realm boundary.
use super::webgl2_bindings_tests::{check, document};

const SETUP: &str = r#"
    const gl = __stageWebGl2(new OffscreenCanvas(4,4));
    function program(fragment, vertex='#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}') {
        const p = gl.createProgram();
        for (const [type,source] of [[gl.VERTEX_SHADER,vertex],[gl.FRAGMENT_SHADER,fragment]]) {
            const shader=gl.createShader(type); gl.shaderSource(shader,source); gl.compileShader(shader);
            if (!gl.getShaderParameter(shader,gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(shader));
            gl.attachShader(p,shader);
        }
        return p;
    }
    function link(p) {
        gl.linkProgram(p); if (!gl.getProgramParameter(p,gl.LINK_STATUS)) throw Error(gl.getProgramInfoLog(p));
        gl.useProgram(p);
    }
"#;

#[test]
fn webgl2_realm_fragment_outputs_query_real_linked_locations() {
    let (mut context, _host) = document();
    check(&mut context, SETUP);
    check(
        &mut context,
        r#"
        const p=program('#version 300 es\nprecision highp float;layout(location=2) out vec4 third;layout(location=0) out vec4 first;void main(){third=vec4(1);first=vec4(0);}');
        link(p);
        if (gl.getFragDataLocation(p,'third')!==2 || gl.getFragDataLocation(p,'missing')!==-1)
            throw Error('fragment locations not native');
        if (gl.getFragDataLocation(p,'first')!==0) throw Error('first output location');
        if (gl.getFragDataLocation(p,'x\0y')!==-1 || gl.getError()!==gl.INVALID_VALUE) throw Error('embedded NUL passed to driver');
        const peer=__stageWebGl2(new OffscreenCanvas(2,2));
        if (peer.getFragDataLocation(p,'third')!==-1 || peer.getError()!==gl.INVALID_OPERATION) throw Error('peer program accepted');
    "#,
    );
}

#[test]
fn webgl2_realm_unsigned_uniform_offsets_preserve_all_32_bits() {
    let (mut context, _host) = document();
    check(&mut context, SETUP);
    check(
        &mut context,
        r#"
        const p = program('#version 300 es\nprecision highp float;uniform highp uvec4 data;out vec4 color;void main(){color=vec4(data)/float(4294967295u);}');
        link(p);
        const location=gl.getUniformLocation(p,'data');
        const values=new Uint32Array(10000);
        values.set([0xffffffff,0x80000000,0x7fffffff,3],9996);
        // Native command limits apply to the selected range, not the source view.
        gl.uniform4uiv(location,values,9996,4);
        const actual=gl.getUniform(p,location);
        if (!(actual instanceof Uint32Array) || [...actual].join() !== '4294967295,2147483648,2147483647,3')
            throw Error('unsigned uniform domain lost');
        gl.uniform4uiv(location,values,9999,4);
        if (gl.getError() !== gl.INVALID_VALUE) throw Error('bad source range accepted');
        if ([...gl.getUniform(p,location)].join() !== [...actual].join()) throw Error('failed range changed uniform');
        if (gl.getError() !== 0) throw Error('unexpected GPU error');
    "#,
    );
}

#[test]
fn webgl2_realm_non_square_matrix_offsets_and_integer_attribute_results_are_typed() {
    let (mut context, _host) = document();
    check(&mut context, SETUP);
    check(
        &mut context,
        r#"
        const p=program('#version 300 es\nprecision highp float;uniform mat2x3 data;out vec4 color;void main(){color=vec4(data[0],data[1].x);}');
        link(p);
        const location=gl.getUniformLocation(p,'data');
        gl.uniformMatrix2x3fv(location,false,new Float32Array([99,1,2,3,4,5,6,98]),1,6);
        const matrix=gl.getUniform(p,location);
        if (!(matrix instanceof Float32Array) || [...matrix].join()!=='1,2,3,4,5,6') throw Error('matrix selection or order');
        gl.vertexAttribI4i(1,-2147483648,2147483647,-1,0);
        const signed=gl.getVertexAttrib(1,gl.CURRENT_VERTEX_ATTRIB);
        if (!(signed instanceof Int32Array) || [...signed].join()!=='-2147483648,2147483647,-1,0') throw Error('signed attribute domain');
        gl.vertexAttribI4uiv(1,new Uint32Array([0xffffffff,0x80000000,3,4,123]));
        const unsigned=gl.getVertexAttrib(1,gl.CURRENT_VERTEX_ATTRIB);
        if (!(unsigned instanceof Uint32Array) || [...unsigned].join()!=='4294967295,2147483648,3,4') throw Error('unsigned attribute domain');
        if (gl.getError() !== 0) throw Error('unexpected GPU error');
    "#,
    );
}

#[test]
fn webgl2_realm_uniform_block_reflection_keeps_sequences_and_typed_indices_distinct() {
    let (mut context, _host) = document();
    check(&mut context, SETUP);
    check(
        &mut context,
        r#"
        const p=program('#version 300 es\nprecision highp float;layout(std140) uniform Colors {vec4 tint;};out vec4 color;void main(){color=tint;}');
        link(p);
        const block=gl.getUniformBlockIndex(p,'Colors');
        if (block===0xffffffff || gl.getActiveUniformBlockName(p,block)!=='Colors') throw Error('block reflection');
        const indices=gl.getUniformIndices(p,['tint','missing']);
        if (!Array.isArray(indices) || indices.length!==2 || indices[1]!==0xffffffff) throw Error('uniform indices sequence');
        const active=gl.getActiveUniformBlockParameter(p,block,0x8a43);
        if (!(active instanceof Uint32Array) || active.length!==1 || active[0]!==indices[0]) throw Error('block typed indices');
        const offsets=gl.getActiveUniforms(p,[indices[0]],0x8a3b);
        const rowMajor=gl.getActiveUniforms(p,[indices[0]],0x8a3e);
        if (!Array.isArray(offsets) || offsets[0]!==0 || rowMajor[0]!==false) throw Error('uniform property domains');
        gl.uniformBlockBinding(p,block,2);
        if (gl.getActiveUniformBlockParameter(p,block,0x8a3f)!==2) throw Error('native block binding');
        if (gl.getError() !== 0) throw Error('unexpected GPU error');
    "#,
    );
}

#[test]
fn webgl2_realm_failed_active_feedback_deletion_preserves_brand_and_binding() {
    let (mut context, _host) = document();
    check(&mut context, SETUP);
    check(
        &mut context,
        r#"
        const p=program('#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}',
            '#version 300 es\nout float captured;void main(){captured=0.5;gl_Position=vec4(0,0,0,1);}');
        gl.transformFeedbackVaryings(p,['captured'],0x8c8c); link(p);
        const varying=gl.getTransformFeedbackVarying(p,0);
        if (!(varying instanceof WebGLActiveInfo) || varying.name!=='captured' || varying.size!==1) throw Error('varying reflection');
        const tf=gl.createTransformFeedback(); gl.bindTransformFeedback(0x8e22,tf);
        const buffer=gl.createBuffer(); gl.bindBuffer(0x8c8e,buffer); gl.bufferData(0x8c8e,16,gl.STATIC_DRAW);
        gl.bindBufferBase(0x8c8e,0,buffer); gl.beginTransformFeedback(gl.POINTS);
        gl.deleteTransformFeedback(tf);
        if (gl.getError()!==gl.INVALID_OPERATION || !gl.isTransformFeedback(tf) || gl.getParameter(0x8e25)!==tf)
            throw Error('failed deletion poisoned the realm brand');
        gl.endTransformFeedback(); gl.deleteTransformFeedback(tf);
        if (gl.isTransformFeedback(tf) || gl.getParameter(0x8e25)!==null || gl.getError()!==0) throw Error('successful deletion not committed');
    "#,
    );
}
