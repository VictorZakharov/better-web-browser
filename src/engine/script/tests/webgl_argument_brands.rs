use super::webgl_instancing::check;
use super::webgl_lifecycle::run;

const CONVERSIONS: &str = r#"
    const assert = (condition, label) => { if (!condition) throw Error(label); };
    const typeError = (callback, label) => {
        let caught = false;
        try { callback(); } catch (error) { caught = error instanceof TypeError; }
        assert(caught, label);
    };
    const invalid = [{}, 1, 'shader', Object.create(WebGLShader.prototype),
        Object.create(WebGLBuffer.prototype), {type:'WebGLProgram',id:1}];
    for (const object of invalid) {
        typeError(() => gl.isBuffer(object), 'isBuffer brand');
        typeError(() => gl.deleteTexture(object), 'deleteTexture brand');
        typeError(() => gl.bindBuffer(gl.ARRAY_BUFFER, object), 'bindBuffer brand');
        typeError(() => gl.compileShader(object), 'compileShader brand');
        typeError(() => gl.useProgram(object), 'useProgram brand');
        typeError(() => gl.attachShader(program, object), 'attachShader brand');
        typeError(() => gl.attachShader(object, shader), 'attachShader program brand');
        typeError(() => gl.getUniform(program, object), 'getUniform location brand');
        typeError(() => gl.getUniform(object, null), 'getUniform program brand');
        typeError(() => gl.framebufferTexture2D(gl.FRAMEBUFFER,
            gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, object, 0), 'texture attachment brand');
        typeError(() => gl.framebufferRenderbuffer(gl.FRAMEBUFFER,
            gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, object), 'renderbuffer attachment brand');
        typeError(() => gl.uniform4f(object, 1, 0, 0, 1), 'scalar uniform brand');
        typeError(() => gl.uniform2fv(object, [0, 0]), 'vector uniform brand');
        typeError(() => gl.uniformMatrix4fv(object, false, new Float32Array(16)), 'matrix uniform brand');
    }
    for (const object of [null, undefined]) {
        assert(gl.isBuffer(object) === false, 'nullable predicate');
        assert(gl.deleteBuffer(object) === undefined, 'nullable deletion');
        assert(gl.bindTexture(gl.TEXTURE_2D, object) === undefined, 'nullable binding');
        assert(gl.useProgram(object) === undefined, 'nullable current program');
        assert(gl.uniform1f(object, 1) === undefined, 'nullable uniform');
        typeError(() => gl.compileShader(object), 'non-null shader');
        typeError(() => gl.linkProgram(object), 'non-null program');
        typeError(() => gl.getUniform(program, object), 'non-null uniform location');
    }
    typeError(() => gl.isBuffer(shader), 'genuine wrong interface');
    typeError(() => gl.getShaderParameter(program, gl.COMPILE_STATUS), 'genuine program as shader');
    typeError(() => gl.getProgramInfoLog(shader), 'genuine shader as program');
    typeError(() => gl.isBuffer(), 'required argument still required');
"#;

#[test]
fn webgl_resource_interfaces_reject_forged_brands_without_native_errors() {
    check(&format!(
        r#"
        const gl = document.querySelector('canvas').getContext('webgl');
        const program = gl.createProgram(), shader = gl.createShader(gl.VERTEX_SHADER);
        {CONVERSIONS}
        assert(gl.getError() === gl.NO_ERROR, 'conversion must not queue native errors');
        document.querySelector('output').textContent = 'pass';
    "#
    ));
}

#[test]
fn webgl_resource_interface_conversion_still_occurs_while_context_is_lost() {
    run(&format!(
        r#"
        const canvas = document.querySelector('canvas');
        const gl = canvas.getContext('webgl');
        const program = gl.createProgram(), shader = gl.createShader(gl.VERTEX_SHADER);
        gl.getExtension('WEBGL_lose_context').loseContext();
        {CONVERSIONS}
        assert(gl.getError() === gl.CONTEXT_LOST_WEBGL, 'loss error remains first');
        assert(gl.getError() === gl.NO_ERROR, 'IDL exceptions do not queue lost errors');
        canvas.setAttribute('data-result', 'pass');
    "#
    ));
}

#[test]
fn webgl_foreign_resource_is_well_branded_but_rejected_by_context_ownership() {
    check(
        r#"
        const gl = document.querySelector('canvas').getContext('webgl');
        const peer = new OffscreenCanvas(1,1).getContext('webgl');
        const buffer = peer.createBuffer(), shader = peer.createShader(peer.VERTEX_SHADER);
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        if (gl.getError() !== gl.INVALID_OPERATION) throw Error('foreign binding');
        gl.compileShader(shader);
        if (gl.getError() !== gl.INVALID_OPERATION) throw Error('foreign shader');
        if (gl.isBuffer(buffer) || gl.getError() !== gl.NO_ERROR) throw Error('foreign predicate');
        gl.getExtension('WEBGL_lose_context').loseContext();
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.compileShader(shader);
        if (gl.getError() !== gl.CONTEXT_LOST_WEBGL || gl.getError() !== gl.NO_ERROR)
            throw Error('well-branded resources must be ignored after loss');
        document.querySelector('output').textContent = 'pass';
    "#,
    );
}
