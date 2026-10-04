use super::webgl_instancing::check;

#[test]
fn webgl_anisotropy_is_capability_gated_and_preserves_fractional_state() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        assert(gl.getParameter(0x84ff)===null,'maximum hidden before enabling');
        assert(gl.getError()===gl.INVALID_ENUM,'maximum admission');
        assert(gl.getTexParameter(gl.TEXTURE_2D,0x84fe)===null,'texture query admission');
        assert(gl.getError()===gl.INVALID_ENUM,'texture enum hidden');
        gl.texParameterf(gl.TEXTURE_2D,0x84fe,2);
        assert(gl.getError()===gl.INVALID_ENUM,'setter admission');
        const ext=gl.getExtension('EXT_texture_filter_anisotropic');
        assert(ext,'real ANGLE capability');
        assert(typeof EXT_texture_filter_anisotropic==='undefined','no global constructor');
        assert(Object.prototype.toString.call(ext)==='[object EXT_texture_filter_anisotropic]','IDL brand');
        assert(ext===gl.getExtension('ext_TEXTURE_filter_ANISOTROPIC'),'cached identity');
        assert(ext.TEXTURE_MAX_ANISOTROPY_EXT===0x84fe,'texture constant');
        assert(ext.MAX_TEXTURE_MAX_ANISOTROPY_EXT===0x84ff,'limit constant');
        const limit=gl.getParameter(ext.MAX_TEXTURE_MAX_ANISOTROPY_EXT);
        assert(typeof limit==='number'&&limit>=2,'native limit');
        assert(gl.getTexParameter(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT)===1,'default');
        gl.texParameterf(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT,1.5);
        assert(gl.getTexParameter(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT)===1.5,'fraction not truncated');
        gl.texParameteri(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT,2);
        assert(gl.getTexParameter(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT)===2,'integer setter');
        gl.texParameterf(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT,0.5);
        assert(gl.getError()===gl.INVALID_VALUE,'minimum validation');
        assert(gl.getTexParameter(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT)===2,'invalid setter preserves state');
        gl.texParameterf(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT,limit*2);
        // EXT permits clamping, but does not require the queried state to clamp.
        // Sampling uses the smaller of this state and the implementation limit.
        const aboveLimit=gl.getTexParameter(gl.TEXTURE_2D,ext.TEXTURE_MAX_ANISOTROPY_EXT);
        assert(aboveLimit>=limit&&aboveLimit<=limit*2,'above-limit state '+aboveLimit+' maximum '+limit+' error '+gl.getError());
        const other=document.createElement('canvas').getContext('webgl');
        assert(other.getParameter(0x84ff)===null,'peer not implicitly enabled');
        assert(other.getError()===other.INVALID_ENUM,'peer admission');
        assert(gl.getError()===0,'no leaked native errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_fragment_texture_lod_projection_and_gradients_sample_real_mip_levels() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const ext=gl.getExtension('EXT_shader_texture_lod');assert(ext,'native texture LOD capability');
        assert(Object.prototype.toString.call(ext)==='[object EXT_shader_texture_lod]','extension brand');
        assert(typeof EXT_shader_texture_lod==='undefined','no global extension constructor');
        assert(ext===gl.getExtension('ext_SHADER_texture_LOD'),'cached extension object');
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,
            new Uint8Array([255,0,0,255,255,0,0,255,255,0,0,255,255,0,0,255]));
        gl.texImage2D(gl.TEXTURE_2D,1,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([0,255,0,255]));
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST_MIPMAP_NEAREST);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
        const expressions=[
            ['texture2DLodEXT(image,vec2(.5),0.)','255,0,0,255'],
            ['texture2DLodEXT(image,vec2(.5),1.)','0,255,0,255'],
            ['texture2DProjLodEXT(image,vec3(.25,.25,.5),1.)','0,255,0,255'],
            ['texture2DProjLodEXT(image,vec4(.25,.25,0.,.5),1.)','0,255,0,255'],
            ['texture2DGradEXT(image,vec2(.5),vec2(0.),vec2(0.))','255,0,0,255'],
            ['texture2DGradEXT(image,vec2(.5),vec2(1.,0.),vec2(0.,1.))','0,255,0,255'],
            ['texture2DProjGradEXT(image,vec3(.25,.25,.5),vec2(1.,0.),vec2(0.,1.))','0,255,0,255'],
            ['texture2DProjGradEXT(image,vec4(.25,.25,0.,.5),vec2(1.,0.),vec2(0.,1.))','0,255,0,255']
        ];
        for(const [expression,expected] of expressions) {
            const program=gl.createProgram();
            gl.attachShader(program,compile(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
            gl.attachShader(program,compile(gl.FRAGMENT_SHADER,
                '#extension GL_EXT_shader_texture_lod : require\nprecision mediump float;uniform sampler2D image;void main(){gl_FragColor='+expression+';}'));
            gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
            assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
            gl.uniform1i(gl.getUniformLocation(program,'image'),0);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
            const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            assert(String(pixel)===expected,expression+' selected wrong mip: '+pixel);
            assert(gl.getError()===0,expression+' produced native error');
        }
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_cube_texture_lod_and_gradients_sample_native_cube_mips() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        assert(gl.getExtension('EXT_shader_texture_lod'),'texture LOD extension available');
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(0);
        gl.bindTexture(gl.TEXTURE_CUBE_MAP,gl.createTexture());
        for(let face=0;face<6;face++) {
            const target=gl.TEXTURE_CUBE_MAP_POSITIVE_X+face;
            gl.texImage2D(target,0,gl.RGBA,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,
                new Uint8Array([255,0,0,255,255,0,0,255,255,0,0,255,255,0,0,255]));
            gl.texImage2D(target,1,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([0,0,255,255]));
        }
        gl.texParameteri(gl.TEXTURE_CUBE_MAP,gl.TEXTURE_MIN_FILTER,gl.NEAREST_MIPMAP_NEAREST);
        gl.texParameteri(gl.TEXTURE_CUBE_MAP,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
        for(const [expression,expected] of [
            ['textureCubeLodEXT(image,vec3(1.,0.,0.),0.)','255,0,0,255'],
            ['textureCubeLodEXT(image,vec3(1.,0.,0.),1.)','0,0,255,255'],
            ['textureCubeGradEXT(image,vec3(1.,0.,0.),vec3(0.),vec3(0.))','255,0,0,255']]) {
            const program=gl.createProgram();
            gl.attachShader(program,compile(gl.VERTEX_SHADER,'attribute vec2 p;void main(){gl_Position=vec4(p,0.,1.);}'));
            gl.attachShader(program,compile(gl.FRAGMENT_SHADER,
                '#extension GL_EXT_shader_texture_lod : require\nprecision mediump float;uniform samplerCube image;void main(){gl_FragColor='+expression+';}'));
            gl.bindAttribLocation(program,0,'p');gl.linkProgram(program);
            assert(gl.getProgramParameter(program,gl.LINK_STATUS),gl.getProgramInfoLog(program));gl.useProgram(program);
            gl.uniform1i(gl.getUniformLocation(program,'image'),0);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
            const pixel=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            assert(String(pixel)===expected,'cube mip pixels: '+expression+' '+pixel);
        }
        assert(gl.getError()===0,'cube LOD drawing has no errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_texture_lod_builtin_is_shader_and_context_extension_gated() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const shader=gl.createShader(gl.FRAGMENT_SHADER);
        const body='precision mediump float;uniform sampler2D image;void main(){gl_FragColor=texture2DLodEXT(image,vec2(.5),1.);}';
        const compile=source=>{gl.shaderSource(shader,source);gl.compileShader(shader);return gl.getShaderParameter(shader,gl.COMPILE_STATUS)};
        assert(!compile('#extension GL_EXT_shader_texture_lod : require\n'+body),'extension unavailable before request');
        gl.getExtension('EXT_shader_texture_lod');
        assert(!compile(body),'builtin requires a directive');
        assert(compile('#extension GL_EXT_shader_texture_lod : enable\n'+body),'extension and shader directive enabled');
        assert(!compile('#extension GL_EXT_shader_texture_lod : disable\n'+body),'disabled shader builtin');
        assert(gl.getError()===0,'shader compile failures do not queue GL errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}
