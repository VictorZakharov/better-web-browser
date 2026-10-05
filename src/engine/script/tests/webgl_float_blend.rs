//! HDR compositing is verified through real float framebuffer readback.
use super::webgl_instancing::check;

#[test]
fn float_blend_is_implicitly_enabled_for_both_versions_and_preserves_hdr_values() {
    for (api, admission) in [
        ("webgl", "OES_texture_float"),
        ("webgl", "WEBGL_color_buffer_float"),
        ("webgl2", "EXT_color_buffer_float"),
    ] {
        check(&format!(
            r#"
            const gl=document.querySelector('canvas').getContext('{api}');
            const assert=(value,label)=>{{if(!value)throw Error(label)}};
            assert(gl.getSupportedExtensions().includes('EXT_float_blend'),'native availability');
            assert(typeof EXT_float_blend==='undefined','no public constructor');
            assert(gl.getExtension('{admission}'),'renderability dependency');
            // Do not request EXT_float_blend before the draw: the dependency
            // must enable it implicitly, not merely construct an object.
            const two='{api}'==='webgl2';
            const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
            gl.texImage2D(gl.TEXTURE_2D,0,two?gl.RGBA32F:gl.RGBA,1,1,0,gl.RGBA,gl.FLOAT,null);
            const framebuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'real float target');
            const prefix=two?'#version 300 es\n':'';
            const shader=(kind,source)=>{{const s=gl.createShader(kind);gl.shaderSource(s,prefix+source);
                gl.compileShader(s);assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s}};
            const p=gl.createProgram();
            gl.attachShader(p,shader(gl.VERTEX_SHADER,'void main(){{gl_Position=vec4(0,0,0,1);gl_PointSize=1.;}}'));
            gl.attachShader(p,shader(gl.FRAGMENT_SHADER,'precision highp float;'+(two?'out vec4 color;':'')+
                'void main(){{'+(two?'color':'gl_FragColor')+'=vec4(2.,-.5,.25,.5);}}'));
            gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),'link');gl.useProgram(p);
            gl.viewport(0,0,1,1);gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT);
            gl.enable(gl.BLEND);gl.blendFunc(gl.ONE,gl.ONE);
            gl.drawArrays(gl.POINTS,0,1);gl.drawArrays(gl.POINTS,0,1);
            assert(gl.getError()===0,'implicit float blending');
            const pixels=new Float32Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.FLOAT,pixels);
            assert([...pixels].every((v,i)=>Math.abs(v-[4,-1,.5,1][i])<.001),'unclamped additive HDR');
            assert(gl.getError()===0,'readback');
            const ext=gl.getExtension('EXT_float_blend');assert(ext,'explicit retrieval after implicit admission');
            assert(ext===gl.getExtension('ext_FLOAT_BLEND'),'per-context cached identity');
            assert(Object.prototype.toString.call(ext)==='[object EXT_float_blend]','extension brand');
            assert(Object.keys(ext).length===0,'no invented constants or methods');
            document.querySelector('output').textContent='pass';
        "#
        ));
    }
}

#[test]
fn float_blend_alone_does_not_grant_float_renderability() {
    for api in ["webgl", "webgl2"] {
        check(&format!(
            r#"
            const gl=document.querySelector('canvas').getContext('{api}');
            const assert=(v,s)=>{{if(!v)throw Error(s)}};
            assert(gl.getExtension('EXT_float_blend'),'blend extension');
            const t=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,t);
            gl.texImage2D(gl.TEXTURE_2D,0,'{api}'==='webgl2'?gl.RGBA32F:gl.RGBA,
                1,1,0,gl.RGBA,gl.FLOAT,null);
            if ('{api}'==='webgl') assert(gl.getError()===gl.INVALID_ENUM,'float upload still requires OES');
            else {{
                assert(gl.getError()===0,'WebGL2 float storage is core');
                gl.bindFramebuffer(gl.FRAMEBUFFER,gl.createFramebuffer());
                gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,t,0);
                assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE,'color admission separate');
            }}
            document.querySelector('output').textContent='pass';
        "#
        ));
    }
}
