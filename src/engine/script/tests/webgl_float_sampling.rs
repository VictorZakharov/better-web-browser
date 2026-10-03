use super::webgl_instancing::check;

const DRAW: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(v,s)=>{if(!v)throw Error(s)};
    const error=(code,label)=>{const actual=gl.getError();assert(actual===code,label+' GL '+actual)};
    const compile=(kind,source)=>{
        const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
        assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS),gl.getShaderInfoLog(shader));return shader;
    };
    const program=gl.createProgram();
    gl.attachShader(program,compile(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
    gl.attachShader(program,compile(gl.FRAGMENT_SHADER,'precision highp float;uniform sampler2D image;void main(){gl_FragColor=texture2D(image,vec2(.5));}'));
    gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
    assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
    gl.uniform1i(gl.getUniformLocation(program,'image'),0);
    gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
    gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
    gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const draw=()=>{
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'sample draw');
        const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        error(0,'sample read');return [...pixel];
    };
    const near=(actual,expected)=>actual.every((value,index)=>Math.abs(value-expected[index])<=1);
"#;

#[test]
fn webgl_float_and_half_legacy_base_formats_sample_their_specified_channels() {
    check(&format!(
        r#"{DRAW}
        assert(gl.getExtension('OES_texture_float'),'float textures');
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half textures');
        const formats=[
            [gl.RGBA,[.25,.5,.75,.5],[64,128,191,128]],
            [gl.RGB,[.25,.5,.75],[64,128,191,255]],
            [gl.ALPHA,[.5],[0,0,0,128]],
            [gl.LUMINANCE,[.5],[128,128,128,255]],
            [gl.LUMINANCE_ALPHA,[.25,.5],[64,64,64,128]]
        ];
        for(const type of [gl.FLOAT,half.HALF_FLOAT_OES])for(const [format,values,expected] of formats){{
            const data=type===gl.FLOAT ? new Float32Array(values) : new Uint16Array(new Float16Array(values).buffer);
            gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,type,data);
            error(0,'legacy format '+format+' type '+type);
            const actual=draw();assert(near(actual,expected),'legacy sampled '+format+' type '+type+' '+actual);
        }}
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_float_linear_sampling_requires_the_corresponding_author_extension() {
    check(&format!(
        r#"{DRAW}
        assert(gl.getExtension('OES_texture_float'),'float textures');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2,1,0,gl.RGBA,gl.FLOAT,
            new Float32Array([1,0,0,1,0,0,1,1]));error(0,'float upload');
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
        assert(near(draw(),[0,0,0,255]),'float linear texture incomplete before extension');
        assert(gl.getExtension('OES_texture_float_linear'),'float linear capability');
        const actual=draw();assert(near(actual,[128,0,128,255]),'linear interpolation '+actual);
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_half_linear_sampling_and_generated_mips_use_native_half_storage() {
    check(&format!(
        r#"{DRAW}
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half textures');
        const data=new Uint16Array(new Float16Array([1,0,0,1,0,0,1,1,1,0,0,1,0,0,1,1]).buffer);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2,2,0,gl.RGBA,half.HALF_FLOAT_OES,data);
        error(0,'half upload');
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
        assert(near(draw(),[0,0,0,255]),'half linear texture incomplete before extension');
        assert(gl.getParameter(gl.ACTIVE_TEXTURE)===gl.TEXTURE0,'sampling restores active unit');
        assert(gl.getParameter(gl.TEXTURE_BINDING_2D)!==null,'sampling restores texture binding');
        assert(gl.getTexParameter(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER)===gl.LINEAR,'sampling preserves author filter');
        assert(gl.getExtension('OES_texture_half_float_linear'),'half linear capability');
        assert(near(draw(),[128,0,128,255]),'half interpolation');
        gl.generateMipmap(gl.TEXTURE_2D);error(0,'half mip generation');
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR_MIPMAP_LINEAR);
        assert(near(draw(),[128,0,128,255]),'complete generated half mips');
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_private_legacy_storage_does_not_grant_color_attachment_renderability() {
    check(&format!(
        r#"{DRAW}
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half textures');
        const framebuffer=gl.createFramebuffer();
        for(const format of [gl.ALPHA,gl.LUMINANCE,gl.LUMINANCE_ALPHA]){{
            const count=format===gl.LUMINANCE_ALPHA?2:1;
            gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,half.HALF_FLOAT_OES,new Uint16Array(count));
            error(0,'legacy texture allocation');
            gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,gl.getParameter(gl.TEXTURE_BINDING_2D),0);
            error(0,'legacy attachment allowed but incomplete');
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_INCOMPLETE_ATTACHMENT,'legacy storage not color renderable');
            gl.clear(gl.COLOR_BUFFER_BIT);error(gl.INVALID_FRAMEBUFFER_OPERATION,'clear incomplete legacy attachment');
            gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(gl.INVALID_FRAMEBUFFER_OPERATION,'draw incomplete legacy attachment');
            const out=new Float32Array([9,9,9,9]);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,out);error(gl.INVALID_FRAMEBUFFER_OPERATION,'read incomplete legacy attachment');
            assert([...out].every(v=>v===9),'failed legacy read preserves destination');
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        }}
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_texture_subimages_cannot_use_gles3_implicit_format_type_conversion() {
    check(&format!(
        r#"{DRAW}
        assert(gl.getExtension('OES_texture_float'),'float textures');
        const half=gl.getExtension('OES_texture_half_float');assert(half,'half textures');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.LUMINANCE,1,1,0,gl.LUMINANCE,gl.FLOAT,new Float32Array([.5]));
        error(0,'luminance float allocation');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.ALPHA,gl.FLOAT,new Float32Array([1]));
        error(gl.INVALID_OPERATION,'red storage must not alias public alpha and luminance');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.LUMINANCE,half.HALF_FLOAT_OES,new Uint16Array([0x3c00]));
        error(gl.INVALID_OPERATION,'half cannot redefine a float level by subupload');
        assert(near(draw(),[128,128,128,255]),'failed subupload preserves pixels and swizzle');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.LUMINANCE,gl.FLOAT,new Float32Array([.25]));
        error(0,'matching subupload');assert(near(draw(),[64,64,64,255]),'matching subupload samples');
        // Replacing storage must discard the previous private swizzle.
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,0,0,255]));
        error(0,'replace legacy storage');assert(near(draw(),[255,0,0,255]),'replacement restores RGBA channels');
        document.querySelector('output').textContent='pass';
        "#
    ));
}
