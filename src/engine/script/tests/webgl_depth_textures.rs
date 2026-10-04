use super::webgl_instancing::check;

const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const error=(code,label)=>{const actual=gl.getError();assert(actual===code,label+' actual '+actual+' expected '+code)};
    const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const framebuffer=gl.createFramebuffer();
    const color=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,color);
    gl.renderbufferStorage(gl.RENDERBUFFER,gl.RGBA4,1,1);
    const attach=attachment=>{
        gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        for(const point of [gl.DEPTH_ATTACHMENT,gl.STENCIL_ATTACHMENT,gl.DEPTH_STENCIL_ATTACHMENT])
            gl.framebufferTexture2D(gl.FRAMEBUFFER,point,gl.TEXTURE_2D,null,0);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,color);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,attachment,gl.TEXTURE_2D,texture,0);
        error(0,'depth attachment');
        const status=gl.checkFramebufferStatus(gl.FRAMEBUFFER);
        assert(status===gl.FRAMEBUFFER_COMPLETE,'depth completeness '+status);
    };
"#;

#[test]
fn webgl_depth_linear_sampling_interpolates_four_rendered_depth_values() {
    check(&format!(
        r#"{SETUP}
        assert(gl.getExtension('WEBGL_depth_texture'),'depth support');
        gl.renderbufferStorage(gl.RENDERBUFFER,gl.RGBA4,2,2);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,2,2,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        attach(gl.DEPTH_ATTACHMENT);
        gl.enable(gl.SCISSOR_TEST);
        for(const [x,y,value] of [[0,0,.2],[1,0,.6],[0,1,.4],[1,1,.8]]){{
            gl.scissor(x,y,1,1);gl.clearDepth(value);gl.clear(gl.DEPTH_BUFFER_BIT);
        }}
        gl.disable(gl.SCISSOR_TEST);error(0,'render four depths');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
        const program=gl.createProgram();
        for(const [kind,source] of [
            [gl.VERTEX_SHADER,'attribute vec2 p;void main(){{gl_Position=vec4(p,0.,1.);}}'],
            [gl.FRAGMENT_SHADER,'precision highp float;uniform sampler2D image;void main(){{gl_FragColor=vec4(vec3(texture2D(image,vec2(.5)).r),1.);}}']
        ]){{
            const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
            assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS),gl.getShaderInfoLog(shader));gl.attachShader(program,shader);
        }}
        gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
        assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
        gl.uniform1i(gl.getUniformLocation(program,'image'),0);
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'linear depth draw');
        const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        error(0,'linear depth read');
        assert([...pixel].slice(0,3).every(v=>Math.abs(v-127.5)<=1),'genuine interpolated depth '+pixel);
        assert(pixel[3]===255,'opaque shader output');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_depth_texture_is_context_gated_and_exposes_only_its_idl_constant() {
    check(&format!(
        r#"{SETUP}
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        error(gl.INVALID_ENUM,'depth formats disabled');
        const ext=gl.getExtension('WEBGL_depth_texture');assert(ext,'native depth capability');
        assert(gl.getSupportedExtensions().includes('WEBGL_depth_texture'),'advertised support');
        assert(ext.UNSIGNED_INT_24_8_WEBGL===0x84fa,'packed depth stencil token');
        assert(typeof WEBGL_depth_texture==='undefined','no interface constructor');
        assert(Object.prototype.toString.call(ext)==='[object WEBGL_depth_texture]','extension brand');
        assert(ext===gl.getExtension('webgl_DEPTH_TEXTURE'),'case-insensitive cached extension');
        for(const type of [gl.UNSIGNED_SHORT,gl.UNSIGNED_INT]){{
            gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,type,null);
            error(0,'depth allocation');attach(gl.DEPTH_ATTACHMENT);
            assert(gl.getParameter(gl.DEPTH_BITS)>=16,'depth precision');
            assert(gl.getParameter(gl.STENCIL_BITS)===0,'no stencil in pure depth');
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
        }}
        const other=document.createElement('canvas').getContext('webgl');
        other.bindTexture(other.TEXTURE_2D,other.createTexture());
        other.texImage2D(other.TEXTURE_2D,0,other.DEPTH_COMPONENT,1,1,0,other.DEPTH_COMPONENT,other.UNSIGNED_INT,null);
        assert(other.getError()===other.INVALID_ENUM,'peer extension state independent');
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_depth_clear_is_sampled_by_a_real_shader_and_packed_stencil_retains_precision() {
    check(&format!(
        r#"{SETUP}
        const ext=gl.getExtension('WEBGL_depth_texture');assert(ext,'depth capability');
        const shader=(kind,source)=>{{
            const object=gl.createShader(kind);gl.shaderSource(object,source);gl.compileShader(object);
            assert(gl.getShaderParameter(object,gl.COMPILE_STATUS),gl.getShaderInfoLog(object));return object;
        }};
        const program=gl.createProgram();
        gl.attachShader(program,shader(gl.VERTEX_SHADER,'attribute vec2 p;void main(){{gl_Position=vec4(p,0.,1.);}}'));
        gl.attachShader(program,shader(gl.FRAGMENT_SHADER,'precision highp float;uniform sampler2D image;void main(){{gl_FragColor=vec4(vec3(texture2D(image,vec2(.5)).r),1.);}}'));
        gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
        assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
        gl.uniform1i(gl.getUniformLocation(program,'image'),0);
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        const cases=[
            [gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,gl.DEPTH_ATTACHMENT,.25],
            [gl.DEPTH_COMPONENT,gl.UNSIGNED_INT,gl.DEPTH_ATTACHMENT,.5],
            [gl.DEPTH_STENCIL,ext.UNSIGNED_INT_24_8_WEBGL,gl.DEPTH_STENCIL_ATTACHMENT,.75]
        ];
        for(const filter of [gl.NEAREST,gl.LINEAR])for(const [format,type,attachment,value] of cases){{
            gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,filter);
            gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,filter);
            gl.texImage2D(gl.TEXTURE_2D,0,format,1,1,0,format,type,null);error(0,'depth allocation');
            attach(attachment);
            if(format===gl.DEPTH_STENCIL){{
                assert(gl.getParameter(gl.DEPTH_BITS)>=24,'packed depth precision');
                assert(gl.getParameter(gl.STENCIL_BITS)>=8,'packed stencil precision');
            }}
            gl.clearDepth(value);gl.clearStencil(3);gl.clear(gl.DEPTH_BUFFER_BIT|gl.STENCIL_BUFFER_BIT);
            error(0,'depth stencil clear');
            gl.bindFramebuffer(gl.FRAMEBUFFER,null);
            gl.drawArrays(gl.TRIANGLE_STRIP,0,4);error(0,'depth sample draw');
            const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            error(0,'depth sample read');
            assert([...pixel].slice(0,3).every(v=>Math.abs(v-value*255)<=1),'sampled rendered depth '+pixel);
            assert(pixel[3]===255,'sample output alpha');
        }}
        document.querySelector('output').textContent='pass';
        "#
    ));
}

#[test]
fn webgl_depth_textures_reject_data_mips_subuploads_and_framebuffer_copies() {
    check(&format!(
        r#"{SETUP}
        const ext=gl.getExtension('WEBGL_depth_texture');assert(ext,'depth capability');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,new Uint16Array([1]));
        error(gl.INVALID_OPERATION,'render-only texture rejects data');
        gl.texImage2D(gl.TEXTURE_2D,1,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        error(gl.INVALID_OPERATION,'depth must use level zero');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.FLOAT,null);
        error(gl.INVALID_OPERATION,'invalid pure-depth type');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_SHORT,null);
        error(gl.INVALID_OPERATION,'depth type cannot define a color texture');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_STENCIL,1,1,0,gl.DEPTH_STENCIL,gl.UNSIGNED_INT,null);
        error(gl.INVALID_OPERATION,'packed image needs packed type');
        gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        error(0,'valid depth allocation after errors');
        gl.texSubImage2D(gl.TEXTURE_2D,0,0,0,1,1,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        error(gl.INVALID_OPERATION,'depth subupload forbidden');
        gl.generateMipmap(gl.TEXTURE_2D);error(gl.INVALID_OPERATION,'depth mip generation forbidden');
        gl.copyTexImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT,0,0,1,1,0);
        error(gl.INVALID_OPERATION,'depth copy definition forbidden');
        gl.copyTexSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,1,1);
        error(gl.INVALID_OPERATION,'depth subcopy forbidden');
        const cube=gl.createTexture();gl.bindTexture(gl.TEXTURE_CUBE_MAP,cube);
        gl.texImage2D(gl.TEXTURE_CUBE_MAP_POSITIVE_X,0,gl.DEPTH_COMPONENT,1,1,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_SHORT,null);
        error(gl.INVALID_OPERATION,'cube depth forbidden in WebGL1');
        gl.bindTexture(gl.TEXTURE_2D,texture);attach(gl.DEPTH_ATTACHMENT);
        gl.clearDepth(.25);gl.clear(gl.DEPTH_BUFFER_BIT);error(0,'errors preserve valid depth target');
        document.querySelector('output').textContent='pass';
        "#
    ));
}
