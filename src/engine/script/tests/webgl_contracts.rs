//! Error and lifecycle cases which a working textured triangle does not exercise.
use super::*;
fn check(script: &str) {
    let (dom, outcome) = execute_html(&format!("<body><output></output><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}
#[test]
fn webgl_methods_have_idl_arity_brand_checks_and_void_returns() {
    check(
        r#"
        const gl = new OffscreenCanvas(2,2).getContext('webgl');
        const checks = [gl.clearColor.length === 4, gl.clearColor.name === 'clearColor',
            gl.texImage2D.length === 6, gl.drawElements.length === 4,
            gl.clearColor(0,0,0,0) === undefined, gl.clear(gl.COLOR_BUFFER_BIT) === undefined,
            gl.bindBuffer(gl.ARRAY_BUFFER,null) === undefined];
        let count = 0;
        for (const call of [() => gl.clearColor(0), () => gl.readPixels(),
            () => gl.createShader(), () => gl.bindTexture(gl.TEXTURE_2D),
            () => WebGLRenderingContext.prototype.clear.call({},0),
            () => new WebGLBuffer(), () => new WebGLShader(),
            () => new OffscreenCanvas(1,1).getContext('webgl',42)]) {
            try { call(); } catch (error) { if (error instanceof TypeError) count++; }
        }
        checks.push(count === 8, gl.getError() === 0, gl.isBuffer(null) === false,
            Object.keys(gl).length === 0, typeof __hostCall === 'undefined');
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_pixel_store_query_types_and_invalid_enums() {
    check(
        r#"
        const gl = new OffscreenCanvas(2,2).getContext('webgl');
        const checks = [gl.getParameter(gl.COLOR_WRITEMASK).every(Boolean),
            gl.getParameter(gl.COLOR_CLEAR_VALUE) instanceof Float32Array,
            gl.getParameter(gl.VIEWPORT) instanceof Int32Array,
            gl.getParameter(gl.COMPRESSED_TEXTURE_FORMATS) instanceof Uint32Array,
            gl.getParameter(gl.VERSION).startsWith('WebGL 1.0'), gl.getParameter(gl.ACTIVE_TEXTURE) === gl.TEXTURE0];
        gl.pixelStorei(gl.PACK_ALIGNMENT,8); checks.push(gl.getParameter(gl.PACK_ALIGNMENT) === 8);
        gl.pixelStorei(gl.PACK_ALIGNMENT,3); checks.push(gl.getError() === gl.INVALID_VALUE);
        checks.push(gl.getParameter(gl.PACK_ALIGNMENT) === 8);
        gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL,gl.NONE);
        checks.push(gl.getParameter(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL) === gl.NONE);
        gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL,17); checks.push(gl.getError() === gl.INVALID_VALUE);
        gl.getParameter(0xdead); checks.push(gl.getError() === gl.INVALID_ENUM);
        gl.isEnabled(0xdead); checks.push(gl.getError() === gl.INVALID_ENUM);
        gl.hint(gl.GENERATE_MIPMAP_HINT,gl.NICEST); checks.push(gl.getParameter(gl.GENERATE_MIPMAP_HINT) === gl.NICEST);
        gl.hint(0xdead,gl.NICEST); checks.push(gl.getError() === gl.INVALID_ENUM);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_resize_clears_under_author_masks_without_losing_state() {
    check(
        r#"
        const canvas = new OffscreenCanvas(2,2), gl = canvas.getContext('webgl',{preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        gl.enable(gl.SCISSOR_TEST); gl.scissor(1,1,1,1); gl.colorMask(false,false,false,false);
        gl.depthMask(false); gl.clearDepth(0.25); gl.clearStencil(7);
        canvas.width = 3;
        const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        const checks = [[...pixels].join() === '0,0,0,0', gl.isEnabled(gl.SCISSOR_TEST),
            gl.getParameter(gl.COLOR_WRITEMASK).every(value => !value), !gl.getParameter(gl.DEPTH_WRITEMASK),
            [...gl.getParameter(gl.COLOR_CLEAR_VALUE)].join() === '1,0,0,1', gl.getParameter(gl.DEPTH_CLEAR_VALUE) === 0.25,
            gl.getParameter(gl.STENCIL_CLEAR_VALUE) === 7, [...gl.getParameter(gl.VIEWPORT)].join() === '0,0,2,2'];
        gl.colorMask(true,true,true,true); gl.disable(gl.SCISSOR_TEST); gl.clear(gl.COLOR_BUFFER_BIT);
        gl.readPixels(2,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels); checks.push([...pixels].join() === '255,0,0,255');
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_opaque_default_buffer_ignores_fragment_alpha() {
    check(
        r#"
        const canvas = new OffscreenCanvas(2,2), gl = canvas.getContext('webgl',{alpha:false});
        const checks = [gl.getContextAttributes().alpha === false, gl.getParameter(gl.ALPHA_BITS) === 0,
            [...gl.getParameter(gl.COLOR_CLEAR_VALUE)].join() === '0,0,0,0'];
        gl.clearColor(1,0,0,0); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        checks.push([...pixels].join() === '255,0,0,255');
        const texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.copyTexImage2D(gl.TEXTURE_2D,0,gl.RGB,0,0,1,1,0);
        const framebuffer = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        checks.push([...pixels].join() === '255,0,0,255', gl.getError() === 0);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_framebuffer_renderbuffer_storage_and_detachment() {
    check(
        r#"
        const gl = new OffscreenCanvas(2,2).getContext('webgl');
        const framebuffer = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        const color = gl.createRenderbuffer(); gl.bindRenderbuffer(gl.RENDERBUFFER,color);
        gl.renderbufferStorage(gl.RENDERBUFFER,gl.RGBA4,2,2);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,color);
        const checks = [gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE,
            gl.getRenderbufferParameter(gl.RENDERBUFFER,gl.RENDERBUFFER_WIDTH) === 2,
            gl.getRenderbufferParameter(gl.RENDERBUFFER,gl.RENDERBUFFER_HEIGHT) === 2,
            gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME) === color];
        gl.clearColor(0,0,1,1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        checks.push([...pixels].join() === '0,0,255,255');
        gl.deleteRenderbuffer(color);
        checks.push(gl.getParameter(gl.RENDERBUFFER_BINDING) === null,
            gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE);
        gl.deleteFramebuffer(framebuffer); checks.push(gl.getParameter(gl.FRAMEBUFFER_BINDING) === null, gl.getError() === 0);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_offscreen_transfer_resets_only_the_drawing_buffer() {
    check(
        r#"
        const canvas = new OffscreenCanvas(2,2), gl = canvas.getContext('webgl',{preserveDrawingBuffer:true});
        gl.clearColor(1,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        const texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D,texture);
        const bitmap = canvas.transferToImageBitmap();
        const target = new OffscreenCanvas(2,2); target.getContext('2d').drawImage(bitmap,0,0);
        const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        const checks = [[...target.getContext('2d').getImageData(0,0,1,1).data].join() === '255,0,0,255',
            [...pixels].join() === '0,0,0,0', gl.getParameter(gl.TEXTURE_BINDING_2D) === texture];
        let error = ''; try { structuredClone(canvas,{transfer:[canvas]}); } catch (e) { error = e.name; }
        checks.push(error === 'InvalidStateError', canvas.width === 2, gl.getError() === 0);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_readback_preserves_outside_pixels_padding_and_tail() {
    check(
        r#"
        const gl = new OffscreenCanvas(1,2).getContext('webgl');
        gl.clearColor(1,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        gl.pixelStorei(gl.PACK_ALIGNMENT,8);
        const padded = new Uint8Array(16); padded.fill(77);
        gl.readPixels(0,0,1,2,gl.RGBA,gl.UNSIGNED_BYTE,padded);
        const checks = [[...padded].join() === '255,0,0,255,77,77,77,77,255,0,0,255,77,77,77,77'];
        gl.pixelStorei(gl.PACK_ALIGNMENT,4);
        const outside = new Uint8ClampedArray(12); outside.fill(77);
        gl.readPixels(-1,0,3,1,gl.RGBA,gl.UNSIGNED_BYTE,outside);
        checks.push([...outside].join() === '77,77,77,77,255,0,0,255,77,77,77,77', gl.getError() === 0);
        gl.readPixels(0,0,1,1,gl.RGB,gl.UNSIGNED_BYTE,padded);
        checks.push(gl.getError() === gl.INVALID_OPERATION,
            gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_FORMAT) === gl.RGBA,
            gl.getParameter(gl.IMPLEMENTATION_COLOR_READ_TYPE) === gl.UNSIGNED_BYTE);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,null);
        checks.push(gl.getError() === gl.INVALID_VALUE);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_creation_failure_is_actionable_without_claiming_a_context() {
    check(
        r#"
        const canvas = new OffscreenCanvas(1,1);
        let event;
        canvas.addEventListener('webglcontextcreationerror', value => event = value);
        const absent = canvas.getContext('webgl',{failIfMajorPerformanceCaveat:true});
        const checks = [absent === null, event instanceof WebGLContextEvent,
            event.statusMessage.includes('software'), !event.bubbles,
            new WebGLContextEvent('probe',{statusMessage:'example'}).statusMessage === 'example',
            canvas.getContext('2d') !== null];
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
#[test]
fn webgl_shader_and_program_deletion_wait_for_last_native_reference() {
    check(
        r#"
        const gl = new OffscreenCanvas(2,2).getContext('webgl');
        const v = gl.createShader(gl.VERTEX_SHADER), f = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(v,'void main(){gl_Position=vec4(0.0);}'); gl.compileShader(v);
        gl.shaderSource(f,'precision mediump float; uniform vec4 color; void main(){gl_FragColor=color;}'); gl.compileShader(f);
        const p = gl.createProgram(); gl.attachShader(p,v); gl.attachShader(p,f); gl.linkProgram(p); gl.useProgram(p);
        const color = gl.getUniformLocation(p,'color'); gl.uniform4f(color,1,0,0,1);
        gl.deleteShader(v); gl.deleteShader(v);
        const checks = [gl.isShader(v), gl.getShaderParameter(v,gl.DELETE_STATUS), gl.getAttachedShaders(p).includes(v)];
        gl.detachShader(p,v); checks.push(!gl.isShader(v));
        gl.deleteProgram(p); checks.push(gl.isProgram(p),gl.getProgramParameter(p,gl.DELETE_STATUS));
        gl.uniform4f(color,0,1,0,1); checks.push([...gl.getUniform(p,color)].join() === '0,1,0,1');
        gl.useProgram(null); checks.push(!gl.isProgram(p), gl.getError() === 0);
        document.querySelector('output').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
