use super::webgl_instancing::check;

#[test]
fn webgl_256_character_identifier_preserves_existing_angle_prefix() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const attribute='_u'+'a'.repeat(254),uniform='_u'+'b'.repeat(254);
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        const p=gl.createProgram();
        gl.attachShader(p,compile(gl.VERTEX_SHADER,'attribute vec4 '+attribute+';void main(){gl_Position='+attribute+';}'));
        gl.attachShader(p,compile(gl.FRAGMENT_SHADER,'precision mediump float;uniform vec4 '+uniform+';void main(){gl_FragColor='+uniform+';}'));
        gl.bindAttribLocation(p,3,attribute);gl.linkProgram(p);
        assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));gl.useProgram(p);
        assert(gl.getAttribLocation(p,attribute)===3 && gl.getActiveAttrib(p,0).name===attribute,'long attribute reflection');
        assert(gl.getActiveUniform(p,0).name===uniform,'long prefixed uniform reflection');
        const l=gl.getUniformLocation(p,uniform);assert(l,'long uniform location');
        gl.uniform4f(l,0,1,0,1);assert(String(gl.getUniform(p,l))==='0,1,0,1','long uniform upload');
        assert(gl.getError()===0,'maximum identifier queries');
        assert(gl.getUniformLocation(p,uniform+'x')===null && gl.getError()===gl.INVALID_VALUE,'location length cap');
        gl.bindAttribLocation(p,0,attribute+'x');assert(gl.getError()===gl.INVALID_VALUE,'binding length cap');
        assert(gl.getAttribLocation(p,attribute+'x')===-1 && gl.getError()===gl.INVALID_VALUE,'attribute length cap');
        const tooLong=gl.createShader(gl.VERTEX_SHADER);
        gl.shaderSource(tooLong,'attribute vec4 '+attribute+'x;void main(){gl_Position='+attribute+'x;}');gl.compileShader(tooLong);
        assert(!gl.getShaderParameter(tooLong,gl.COMPILE_STATUS),'shader token length cap');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_validator_rejects_non_webgl_languages_and_dynamic_loop_bounds() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const invalid=[
            '#version 300 es\nvoid main(){gl_Position=vec4(0.);}',
            'uniform int limit;void main(){float sum=0.;for(int i=0;i<limit;i++){sum+=1.;}gl_Position=vec4(sum);}',
            'void main(){gl_Position=vec4(this_is_not_a_variable);}',
            '#extension GL_DOES_NOT_EXIST : require\nvoid main(){gl_Position=vec4(0.);}'
        ];
        const shader=gl.createShader(gl.VERTEX_SHADER);
        for(const source of invalid) {
            gl.shaderSource(shader,source);gl.compileShader(shader);
            assert(!gl.getShaderParameter(shader,gl.COMPILE_STATUS),'invalid shader compiled');
            assert(gl.getShaderInfoLog(shader).length>0,'actionable compiler diagnostic');
            assert(gl.getShaderSource(shader)===source,'author source retained after failure');
            assert(gl.getError()===0,'compiler failure is not a GL API error');
        }
        gl.shaderSource(shader,'void main(){float sum=0.;for(int i=0;i<3;i++){sum+=1.;}gl_Position=vec4(sum);}');
        gl.compileShader(shader);
        assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS),'constant loop is valid WebGL');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_validation_failure_replaces_previous_compile_and_link_status() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const vertex=gl.createShader(gl.VERTEX_SHADER), fragment=gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(vertex,'void main(){gl_Position=vec4(0.);}');gl.compileShader(vertex);
        gl.shaderSource(fragment,'precision mediump float;void main(){gl_FragColor=vec4(1.);}');gl.compileShader(fragment);
        const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,fragment);gl.linkProgram(p);
        assert(gl.getProgramParameter(p,gl.LINK_STATUS),'initial successful link');
        gl.shaderSource(vertex,'#version 300 es\nvoid main(){gl_Position=vec4(0.);}');gl.compileShader(vertex);
        assert(!gl.getShaderParameter(vertex,gl.COMPILE_STATUS),'failed recompile status');
        const log=gl.getShaderInfoLog(vertex);
        gl.shaderSource(vertex,'void main(){gl_Position=vec4(1.);}');
        assert(!gl.getShaderParameter(vertex,gl.COMPILE_STATUS),'source assignment does not compile');
        assert(gl.getShaderInfoLog(vertex)===log,'source assignment retains last compiler log');
        gl.linkProgram(p);assert(!gl.getProgramParameter(p,gl.LINK_STATUS),'link cannot reuse invalid compile');
        gl.compileShader(vertex);gl.linkProgram(p);
        assert(gl.getShaderParameter(vertex,gl.COMPILE_STATUS) && gl.getProgramParameter(p,gl.LINK_STATUS),'recover after validation failure');
        assert(gl.getError()===0,'compilation lifecycle has no API errors');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_translated_uniform_arrays_struct_fields_and_prefix_names_remain_public() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const compile=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
        const p=gl.createProgram();
        gl.attachShader(p,compile(gl.VERTEX_SHADER,'attribute vec2 _uposition;uniform float values[2];void main(){gl_Position=vec4(_uposition+vec2(values[0]+values[1]),0.,1.);}'));
        gl.attachShader(p,compile(gl.FRAGMENT_SHADER,'precision mediump float;struct Color{vec4 _utint;};uniform Color paint;void main(){gl_FragColor=paint._utint;}'));
        gl.bindAttribLocation(p,2,'_uposition');gl.linkProgram(p);
        assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));gl.useProgram(p);
        assert(gl.getAttribLocation(p,'_uposition')===2,'attribute name and explicit binding');
        assert(gl.getActiveAttrib(p,0).name==='_uposition','attribute reflection strips exactly one prefix');
        const names=[];
        for(let i=0;i<gl.getProgramParameter(p,gl.ACTIVE_UNIFORMS);i++)names.push(gl.getActiveUniform(p,i).name);
        assert(names.includes('values[0]') && names.includes('paint._utint'),'arrays and struct reflection');
        const first=gl.getUniformLocation(p,'values[0]'),second=gl.getUniformLocation(p,'values[1]');
        const color=gl.getUniformLocation(p,'paint._utint');
        assert(first && second && color,'translated uniform locations');
        gl.uniform1f(first,0);gl.uniform1f(second,0);gl.uniform4f(color,0,1,0,1);
        assert(String(gl.getUniform(p,color))==='0,1,0,1','struct field uniform readback');
        gl.bindBuffer(gl.ARRAY_BUFFER,gl.createBuffer());
        gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),gl.STATIC_DRAW);
        gl.vertexAttribPointer(2,2,gl.FLOAT,false,0,0);gl.enableVertexAttribArray(2);
        gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
        const pixels=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,255,0,255','translated names feed real pixels');
        assert(gl.getError()===0,'reflection and draw have no error');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_shader_extension_is_disabled_until_requested_and_after_restore() {
    check(
        r#"
        const canvas=document.querySelector('canvas'), gl=canvas.getContext('webgl');
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const source='#extension GL_OES_standard_derivatives : require\nprecision mediump float;void main(){gl_FragColor=vec4(dFdx(gl_FragCoord.x));}';
        const compile=()=>{const s=gl.createShader(gl.FRAGMENT_SHADER);gl.shaderSource(s,source);gl.compileShader(s);return s;};
        assert(!gl.getShaderParameter(compile(),gl.COMPILE_STATUS),'extension disabled initially');
        gl.getExtension('OES_standard_derivatives');
        assert(gl.getShaderParameter(compile(),gl.COMPILE_STATUS),'extension enabled explicitly');
        const loss=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();setTimeout(()=>loss.restoreContext(),0)});
        canvas.addEventListener('webglcontextrestored',()=>{
            assert(!gl.getShaderParameter(compile(),gl.COMPILE_STATUS),'restoration resets compiler extension state');
            gl.getExtension('OES_standard_derivatives');
            assert(gl.getShaderParameter(compile(),gl.COMPILE_STATUS),'restored extension enables compiler again');
            document.querySelector('output').textContent='pass';
        });
        loss.loseContext();
    "#,
    );
}
