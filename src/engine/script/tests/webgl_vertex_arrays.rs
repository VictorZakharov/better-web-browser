use super::webgl_instancing::{SETUP, check};

#[test]
fn webgl_vertex_arrays_capture_elements_attributes_and_instance_divisors() {
    check(&format!(
        r#"{SETUP}
        const vao=gl.getExtension('OES_vertex_array_object');
        assert(vao && gl.getSupportedExtensions().includes('OES_vertex_array_object'),'native vertex arrays');
        assert(gl.getParameter(vao.VERTEX_ARRAY_BINDING_OES)===null,'default array');
        const first=vao.createVertexArrayOES(), second=vao.createVertexArrayOES();
        vao.bindVertexArrayOES(first);
        const index=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,index);
        gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint8Array([0,1,2,3]),gl.STATIC_DRAW);
        gl.bindBuffer(gl.ARRAY_BUFFER,vertices);gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        gl.bindBuffer(gl.ARRAY_BUFFER,offsets);gl.vertexAttribPointer(1,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(1);
        ext.vertexAttribDivisorANGLE(1,1);
        vao.bindVertexArrayOES(second);
        assert(gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING)===null,'fresh element binding');
        assert(gl.getParameter(gl.ARRAY_BUFFER_BINDING)===offsets,'global array buffer preserved');
        assert(gl.getVertexAttrib(1,ext.VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE)===0,'fresh divisor');
        assert(!gl.getVertexAttrib(0,gl.VERTEX_ATTRIB_ARRAY_ENABLED),'fresh enable');
        vao.bindVertexArrayOES(first);
        assert(gl.getParameter(vao.VERTEX_ARRAY_BINDING_OES)===first,'object query');
        assert(gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING)===index,'saved element binding');
        assert(gl.getVertexAttrib(1,ext.VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE)===1,'saved divisor');
        ext.drawElementsInstancedANGLE(gl.TRIANGLE_STRIP,4,gl.UNSIGNED_BYTE,0,2);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255' && pixel(6,1)==='0,255,0,255','saved native array renders');
        vao.bindVertexArrayOES(null);
        assert(gl.getVertexAttrib(1,gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING)===offsets,'pre-extension default attributes preserved');
        assert(gl.getVertexAttrib(1,ext.VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE)===1,'pre-extension default divisor preserved');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_deleted_buffer_remains_drawable_through_inactive_vertex_array() {
    check(&format!(
        r#"{SETUP}
        const vao=gl.getExtension('OES_vertex_array_object'), saved=vao.createVertexArrayOES();
        vao.bindVertexArrayOES(saved);
        gl.bindBuffer(gl.ARRAY_BUFFER,vertices);gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        gl.bindBuffer(gl.ARRAY_BUFFER,offsets);gl.vertexAttribPointer(1,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(1);
        ext.vertexAttribDivisorANGLE(1,1);
        vao.bindVertexArrayOES(null);
        gl.deleteBuffer(vertices);
        assert(!gl.isBuffer(vertices),'deleted public buffer');
        assert(gl.getVertexAttrib(0,gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING)===null,'current attribute detached');
        assert(gl.getVertexAttrib(0,gl.VERTEX_ATTRIB_ARRAY_SIZE)===2,'detached attribute format retained');
        gl.bindBuffer(gl.ARRAY_BUFFER,vertices);assert(gl.getError()===gl.INVALID_OPERATION,'cannot bind deleted public buffer');
        vao.bindVertexArrayOES(saved);
        assert(gl.getVertexAttrib(0,gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING)===vertices,'inactive array retained reference');
        ext.drawArraysInstancedANGLE(gl.TRIANGLE_STRIP,0,4,2);
        assert(gl.getError()===0 && pixel(1,1)==='0,255,0,255' && pixel(6,1)==='0,255,0,255','retained driver and CPU storage');
        vao.deleteVertexArrayOES(saved);
        assert(!vao.isVertexArrayOES(saved) && gl.getParameter(vao.VERTEX_ARRAY_BINDING_OES)===null,'bound deletion restores default');
        assert(gl.getError()===0,'release last deleted-buffer reference');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_vertex_array_wrong_context_deleted_and_wrong_brand_rejected() {
    check(&format!(
        r#"{SETUP}
        const vao=gl.getExtension('OES_vertex_array_object'), saved=vao.createVertexArrayOES();
        const peer=document.createElement('canvas').getContext('webgl'), foreign=peer.getExtension('OES_vertex_array_object').createVertexArrayOES();
        vao.bindVertexArrayOES(saved);assert(vao.isVertexArrayOES(saved),'bound valid');
        vao.bindVertexArrayOES(foreign);assert(gl.getError()===gl.INVALID_OPERATION,'foreign binding');
        assert(!vao.isVertexArrayOES(foreign),'foreign predicate');
        assert(gl.getParameter(vao.VERTEX_ARRAY_BINDING_OES)===saved,'failed bind preserves state');
        vao.deleteVertexArrayOES(foreign);assert(gl.getError()===gl.INVALID_OPERATION,'foreign delete');
        vao.deleteVertexArrayOES(saved);vao.deleteVertexArrayOES(saved);
        assert(gl.getError()===0,'idempotent deletion');
        vao.bindVertexArrayOES(saved);assert(gl.getError()===gl.INVALID_OPERATION,'deleted binding');
        let throws=0;
        for(const call of [()=>vao.bindVertexArrayOES(gl.createBuffer()),()=>vao.bindVertexArrayOES(),()=>vao.createVertexArrayOES.call({{}})])
            try{{call()}}catch(e){{if(e instanceof TypeError)throws++}}
        assert(throws===3,'brands and arity');
        assert(typeof globalThis.OES_vertex_array_object==='undefined' && typeof globalThis.WebGLVertexArrayObjectOES==='undefined','legacy no globals');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
