//! Genuine drawing and typed-resource contracts, not just HTML5test feature probes.
use super::*;

fn check(script: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<body><output id='result'></output><script>{script}</script></body>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}
#[test]
#[cfg(windows)]
fn webgl_indexed_textured_geometry_matches_reference_fixture_contracts() {
    let (dom, outcome) = execute_html(include_str!(
        "../../../../tests/fixtures/webgl-rendering.html"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let results = dom.elements_named("pre").next().unwrap().text_content();
    assert!(
        !results.contains("FAIL") && !results.contains("ERROR"),
        "{results}"
    );
    assert_eq!(results.matches("PASS").count(), 18, "{results}");
}

#[test]
#[cfg(windows)]
fn webgl_clear_readback_canvas_copy_and_resize() {
    check(
        r#"
        const canvas = document.createElement('canvas'); canvas.width = 4; canvas.height = 4;
        const gl = canvas.getContext('webgl', {preserveDrawingBuffer:true});
        if (!gl) throw new Error('real ANGLE context required');
        const checks = [gl instanceof WebGLRenderingContext, gl.canvas === canvas,
            canvas.getContext('experimental-webgl') === gl, canvas.getContext('2d') === null,
            canvas.getContext('webgl2') === null, gl.drawingBufferWidth === 4,
            gl.getContextAttributes().antialias === false,
            gl.getSupportedExtensions().length === 0, gl.getExtension('FAKE_extension') === null];
        let illegal = false; try { new WebGLRenderingContext(); } catch (e) { illegal = e instanceof TypeError; }
        checks.push(illegal);
        gl.clearColor(0, 1, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixels = new Uint8Array(4); gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        checks.push([...pixels].join() === '0,255,0,255', gl.getError() === gl.NO_ERROR);
        const copy = document.createElement('canvas'); copy.width = 4; copy.height = 4;
        copy.getContext('2d').drawImage(canvas, 0, 0);
        checks.push([...copy.getContext('2d').getImageData(0, 0, 1, 1).data].join() === '0,255,0,255');
        gl.viewport(1, 2, 3, 4); const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        canvas.width = 8;
        checks.push(gl.drawingBufferWidth === 8, gl.getParameter(gl.ARRAY_BUFFER_BINDING) === buffer,
            [...gl.getParameter(gl.VIEWPORT)].join() === '1,2,3,4');
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        checks.push([...pixels].join() === '0,0,0,0');
        document.getElementById('result').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}

#[test]
#[cfg(windows)]
fn webgl_shader_triangle_uniforms_and_checked_vertex_ranges() {
    check(
        r#"
        const canvas = new OffscreenCanvas(8, 8);
        const gl = canvas.getContext('webgl', {preserveDrawingBuffer:true});
        const vertex = gl.createShader(gl.VERTEX_SHADER), fragment = gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(vertex, 'attribute vec2 position; void main(){gl_Position=vec4(position,0.0,1.0);}');
        gl.shaderSource(fragment, 'precision mediump float; uniform vec4 color; void main(){gl_FragColor=color;}');
        gl.compileShader(vertex); gl.compileShader(fragment);
        const checks = [gl.getShaderParameter(vertex, gl.COMPILE_STATUS), gl.getShaderParameter(fragment, gl.COMPILE_STATUS)];
        const program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment);
        gl.linkProgram(program); checks.push(gl.getProgramParameter(program, gl.LINK_STATUS)); gl.useProgram(program);
        const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1, 1,-1, 0,1]), gl.STATIC_DRAW);
        const position = gl.getAttribLocation(program, 'position'); gl.enableVertexAttribArray(position);
        gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
        const color = gl.getUniformLocation(program, 'color'); gl.uniform4f(color, 1, 0, 0, 1);
        checks.push(gl.getUniformLocation(program, 'color') === color, [...gl.getUniform(program, color)].join() === '1,0,0,1');
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        const pixels = new Uint8Array(4); gl.readPixels(4, 3, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        checks.push([...pixels].join() === '255,0,0,255', gl.getError() === gl.NO_ERROR);
        gl.drawArrays(gl.TRIANGLES, 0, 4); checks.push(gl.getError() === gl.INVALID_OPERATION);
        checks.push(gl.getActiveAttrib(program, 0) instanceof WebGLActiveInfo,
            gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER, gl.MEDIUM_FLOAT) instanceof WebGLShaderPrecisionFormat,
            gl.getAttachedShaders(program).includes(vertex));
        const peer = new OffscreenCanvas(1, 1).getContext('webgl'); peer.bindBuffer(peer.ARRAY_BUFFER, buffer);
        checks.push(peer.getError() === peer.INVALID_OPERATION);
        gl.deleteBuffer(buffer); gl.deleteBuffer(buffer); checks.push(!gl.isBuffer(buffer), gl.getError() === gl.NO_ERROR);
        document.getElementById('result').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}

#[test]
#[cfg(windows)]
fn webgl_texture_framebuffer_upload_and_bounded_readback() {
    check(
        r#"
        const gl = new OffscreenCanvas(4, 4).getContext('webgl');
        const texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 2, 2, 0, gl.RGBA, gl.UNSIGNED_BYTE,
            new Uint8Array([255,0,0,255, 0,255,0,255, 0,0,255,255, 255,255,255,255]));
        const framebuffer = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
        const checks = [gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE,
            gl.getParameter(gl.FRAMEBUFFER_BINDING) === framebuffer,
            gl.getFramebufferAttachmentParameter(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME) === texture];
        const pixels = new Uint8Array(16); gl.readPixels(0, 0, 2, 2, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        checks.push([...pixels].join() === '255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255', gl.getError() === 0);
        const short = new Uint8Array(1); short[0] = 37;
        gl.readPixels(0, 0, 2, 2, gl.RGBA, gl.UNSIGNED_BYTE, short);
        checks.push(gl.getError() === gl.INVALID_OPERATION, short[0] === 37);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 2, 2, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(1));
        checks.push(gl.getError() === gl.INVALID_OPERATION);
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
        checks.push(gl.getError() === gl.INVALID_OPERATION, gl.getParameter(gl.FRAMEBUFFER_BINDING) === null);
        gl.deleteFramebuffer(framebuffer); gl.deleteTexture(texture);
        checks.push(!gl.isFramebuffer(framebuffer), !gl.isTexture(texture));
        document.getElementById('result').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}

#[test]
#[cfg(windows)]
fn webgl_image_upload_flip_and_premultiplication() {
    check(
        r#"
        const gl = new OffscreenCanvas(2, 2).getContext('webgl');
        const texture = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        const image = new ImageData(new Uint8ClampedArray([200,0,0,128, 0,200,0,255]), 1, 2);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true); gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
        const framebuffer = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
        const pixels = new Uint8Array(8); gl.readPixels(0, 0, 1, 2, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        const checks = [[...pixels].join() === '0,200,0,255,100,0,0,128',
            gl.getParameter(gl.UNPACK_FLIP_Y_WEBGL) === true, gl.getParameter(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL) === true, gl.getError() === 0];
        document.getElementById('result').textContent = checks.every(Boolean) ? 'pass' : checks.join();
    "#,
    );
}
