use super::*;

pub(super) fn check(code: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<canvas width=8 height=4></canvas><output></output><script>{code}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}

pub(super) const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const ext=gl.getExtension('ANGLE_instanced_arrays');
    assert(ext && gl.getSupportedExtensions().includes('ANGLE_instanced_arrays'),'native capability');
    const shader=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
    const program=gl.createProgram();
    gl.attachShader(program,shader(gl.VERTEX_SHADER,'attribute vec2 position;attribute vec2 offset;void main(){gl_Position=vec4(position+offset,0.,1.);}'));
    gl.attachShader(program,shader(gl.FRAGMENT_SHADER,'precision mediump float;void main(){gl_FragColor=vec4(0.,1.,0.,1.);}'));
    gl.bindAttribLocation(program,0,'position');gl.bindAttribLocation(program,1,'offset');
    gl.linkProgram(program);assert(gl.getProgramParameter(program,gl.LINK_STATUS),'link');gl.useProgram(program);
    const vertices=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,vertices);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-.5,-1,.5,-1,-.5,1,.5,1]),gl.STATIC_DRAW);
    gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
    const offsets=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,offsets);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-.5,0,.5,0]),gl.STATIC_DRAW);
    gl.vertexAttribPointer(1,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(1);
    ext.vertexAttribDivisorANGLE(1,1);
    const pixel=(x,y)=>{const p=new Uint8Array(4);gl.readPixels(x,y,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);return [...p].join();};
    assert(gl.getError()===0,'setup error');
"#;

#[test]
fn webgl_null_attribute_pointer_is_valid_but_enabled_null_arrays_block_every_draw() {
    check(&format!(
        r#"{SETUP}
        gl.bindBuffer(gl.ARRAY_BUFFER,null);
        gl.vertexAttribPointer(2,3,gl.FLOAT,true,16,0);
        assert(gl.getError()===0,'null buffer with zero offset is valid');
        assert(gl.getVertexAttrib(2,gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING)===null,'null binding');
        assert(gl.getVertexAttrib(2,gl.VERTEX_ATTRIB_ARRAY_SIZE)===3,'size retained');
        assert(gl.getVertexAttrib(2,gl.VERTEX_ATTRIB_ARRAY_NORMALIZED)===true,'normalization retained');
        assert(gl.getVertexAttrib(2,gl.VERTEX_ATTRIB_ARRAY_STRIDE)===16,'stride retained');
        gl.vertexAttribPointer(2,3,gl.FLOAT,false,0,4);
        assert(gl.getError()===gl.INVALID_OPERATION,'nonzero client pointer rejected');
        assert(gl.getVertexAttrib(2,gl.VERTEX_ATTRIB_ARRAY_NORMALIZED)===true,'failure preserves state');
        gl.enableVertexAttribArray(2);
        const indices=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,indices);
        gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint8Array([0,1,2]),gl.STATIC_DRAW);
        for (const draw of [()=>gl.drawArrays(gl.TRIANGLES,0,3),
            ()=>gl.drawElements(gl.TRIANGLES,3,gl.UNSIGNED_BYTE,0),
            ()=>ext.drawArraysInstancedANGLE(gl.TRIANGLES,0,3,1),
            ()=>ext.drawElementsInstancedANGLE(gl.TRIANGLES,3,gl.UNSIGNED_BYTE,0,1)]) {{
            draw();assert(gl.getError()===gl.INVALID_OPERATION,'inactive enabled array still requires buffer');
            assert(pixel(1,1)==='0,0,0,0','invalid draw never changes pixels');
        }}
        gl.disableVertexAttribArray(2);
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,2);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255','disabled null array is harmless');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_instanced_arrays_draw_real_geometry_and_query_divisors() {
    check(&format!(
        r#"{SETUP}
        assert(ext===gl.getExtension('angle_INSTANCED_arrays'),'same extension');
        assert(typeof globalThis.ANGLE_instanced_arrays==='undefined','legacy no interface object');
        assert(gl.getVertexAttrib(1,ext.VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE)===1,'divisor query');
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,2);
        assert(gl.getError()===0,'draw error');
        assert(pixel(1,1)==='0,255,0,255' && pixel(6,1)==='0,255,0,255','two native instances');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_instanced_elements_validates_index_and_instance_ranges() {
    check(&format!(
        r#"{SETUP}
        const indices=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,indices);
        gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array([0,1,2,2,1,3]),gl.STATIC_DRAW);
        ext.drawElementsInstancedANGLE(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,2);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255' && pixel(6,1)==='0,255,0,255','indexed instances');
        gl.clear(gl.COLOR_BUFFER_BIT);
        ext.drawElementsInstancedANGLE(gl.TRIANGLES,7,gl.UNSIGNED_SHORT,0,2);
        assert(gl.getError()===gl.INVALID_OPERATION,'index range');
        ext.drawElementsInstancedANGLE(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,1,2);
        assert(gl.getError()===gl.INVALID_OPERATION,'unaligned index');
        ext.drawElementsInstancedANGLE(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,3);
        assert(gl.getError()===gl.INVALID_OPERATION,'instance range');
        assert(pixel(1,1)==='0,0,0,0','rejected draws do not paint');
        ext.vertexAttribDivisorANGLE(1,2);
        ext.drawElementsInstancedANGLE(gl.TRIANGLES,6,gl.UNSIGNED_SHORT,0,4);
        assert(gl.getError()===0 && pixel(6,1)==='0,255,0,255','divisor advances every two instances');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_instanced_draws_fail_closed_on_work_budget_and_all_instanced_attributes() {
    check(&format!(
        r#"{SETUP}
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,-1);
        assert(gl.getError()===gl.INVALID_VALUE,'negative instance count');
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,1000000);
        assert(gl.getError()===gl.INVALID_VALUE,'multiplicative budget');
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,3);
        assert(gl.getError()===gl.INVALID_OPERATION,'array instance range');
        ext.vertexAttribDivisorANGLE(0,1);
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,1);
        assert(gl.getError()===gl.INVALID_OPERATION,'active per vertex attribute required');
        ext.vertexAttribDivisorANGLE(0,0);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255','ordinary draw respects instance zero');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
