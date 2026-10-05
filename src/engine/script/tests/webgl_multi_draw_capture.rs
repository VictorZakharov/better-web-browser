use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        r#"<script>
    const canvas=document.createElement('canvas'),gl=canvas.getContext('webgl2');
    const ext=gl.getExtension('WEBGL_multi_draw');if(!ext)throw Error('multi-draw unavailable');
    const compile=(type,source)=>{{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);
        if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}};
    const program=(source,names,mode)=>{{
        const p=gl.createProgram();gl.attachShader(p,compile(gl.VERTEX_SHADER,source));
        gl.attachShader(p,compile(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){{color=vec4(1);}}'));
        gl.transformFeedbackVaryings(p,names,mode);gl.linkProgram(p);
        if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));gl.useProgram(p);return p;
    }};
    const buffer=(index,size,offset=0,range=size)=>{{const b=gl.createBuffer();
        gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,b);gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,size,gl.STATIC_DRAW);
        gl.bindBufferRange(gl.TRANSFORM_FEEDBACK_BUFFER,index,b,offset,range);return b;}};
    const read=(b,size)=>{{gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,b);const out=new Float32Array(size);
        gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,out);return Array.from(out);}};
    const expect=(condition,label)=>{{if(!condition)throw Error(label)}};
    const clean=()=>expect(gl.getError()===gl.NO_ERROR,'clean native state');
    gl.enable(gl.RASTERIZER_DISCARD);
    {source}
    </script>"#
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

const VERTEX: &str = r#"const vertex='#version 300 es\n#extension GL_ANGLE_multi_draw : require\nout float value;void main(){value=float(gl_DrawID+1);gl_Position=vec4(0,0,0,1);gl_PointSize=1.0;}';"#;

#[test]
fn multi_draw_feedback_capacity_is_atomic_across_subdraws_and_previous_draws() {
    run(&format!(
        r#"{VERTEX}
    program(vertex,['value'],gl.INTERLEAVED_ATTRIBS);const b=buffer(0,8);
    gl.beginTransformFeedback(gl.POINTS);
    ext.multiDrawArraysWEBGL(gl.POINTS,[0,0,0],0,[1,1,1],0,3);
    expect(gl.getError()===gl.INVALID_OPERATION,'aggregate overflow rejected');
    ext.multiDrawArraysWEBGL(gl.POINTS,[0,0],0,[1,1],0,2);clean();
    gl.drawArrays(gl.POINTS,0,1);expect(gl.getError()===gl.INVALID_OPERATION,'ordinary draw after full batch');
    gl.endTransformFeedback();expect(read(b,2).join(',')==='1,2','actual multi-draw captured IDs');clean();
    gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);clean();
    ext.multiDrawArraysWEBGL(gl.POINTS,[0,0],0,[1,1],0,2);
    expect(gl.getError()===gl.INVALID_OPERATION,'prior ordinary write counted');
    ext.multiDrawArraysWEBGL(gl.POINTS,[0],0,[1],0,1);clean();
    gl.endTransformFeedback();expect(read(b,2).join(',')==='1,1','failed batch did not overwrite cursor');clean();
    "#
    ));
}

#[test]
fn multi_draw_feedback_pause_resume_and_new_capture_preserve_cursor_rules() {
    run(&format!(
        r#"{VERTEX}
    program(vertex,['value'],gl.INTERLEAVED_ATTRIBS);const b=buffer(0,12,4,8);
    gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.pauseTransformFeedback();
    ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0,0],0,[1,1],0,[2,2],0,2);clean();
    gl.resumeTransformFeedback();ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0],0,[1],0,[2],0,1);
    expect(gl.getError()===gl.INVALID_OPERATION,'resumed remaining range');
    ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0],0,[1],0,[1],0,1);clean();
    gl.endTransformFeedback();expect(read(b,3).join(',')==='0,1,1','range and paused writes');clean();
    gl.beginTransformFeedback(gl.POINTS);
    ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0],0,[1],0,[2],0,1);clean();
    gl.endTransformFeedback();expect(read(b,3).join(',')==='0,1,1','new capture resets cursor');clean();
    "#
    ));
}

#[test]
fn multi_draw_feedback_rounds_primitives_per_subdraw_and_instance() {
    run(&format!(
        r#"{VERTEX}
    program(vertex,['value'],gl.INTERLEAVED_ATTRIBS);const b=buffer(0,24);
    gl.beginTransformFeedback(gl.TRIANGLES);
    ext.multiDrawArraysInstancedWEBGL(gl.TRIANGLES,[0,0],0,[4,5],0,[1,1],0,2);clean();
    gl.endTransformFeedback();expect(read(b,6).join(',')==='1,1,1,2,2,2','complete primitive capture');clean();
    gl.beginTransformFeedback(gl.TRIANGLES);
    ext.multiDrawArraysInstancedWEBGL(gl.TRIANGLES,[0],0,[3],0,[3],0,1);
    expect(gl.getError()===gl.INVALID_OPERATION,'instance capacity multiplication');
    ext.multiDrawArraysWEBGL(gl.TRIANGLES,[0,0],0,[2,2],0,2);clean();
    gl.drawArrays(gl.TRIANGLES,0,6);clean();gl.endTransformFeedback();
    expect(read(b,6).every(v=>v===1),'incomplete subdraws did not advance cursor');clean();
    "#
    ));
}

#[test]
fn multi_draw_separate_matrix_capture_uses_minimum_binding_capacity() {
    run(r#"
    const vertex='#version 300 es\n#extension GL_ANGLE_multi_draw : require\nout mat2 matrix;out vec3 vector;void main(){matrix=mat2(float(gl_DrawID+1));vector=vec3(2);gl_Position=vec4(0,0,0,1);}';
    program(vertex,['matrix','vector'],gl.SEPARATE_ATTRIBS);
    const matrix=buffer(0,32),vector=buffer(1,12);
    gl.beginTransformFeedback(gl.POINTS);
    ext.multiDrawArraysWEBGL(gl.POINTS,[0,0],0,[1,1],0,2);
    expect(gl.getError()===gl.INVALID_OPERATION,'shorter separate binding');
    ext.multiDrawArraysWEBGL(gl.POINTS,[0],0,[1],0,1);clean();gl.endTransformFeedback();
    expect(read(matrix,8).join(',')==='1,0,0,1,0,0,0,0','matrix component stride');
    expect(read(vector,3).join(',')==='2,2,2','vector component stride');clean();
    "#);
}
