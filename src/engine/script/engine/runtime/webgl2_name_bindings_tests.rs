//! Public bindings distinguish absent reserved locations from invalid names.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn public_location_queries_preserve_reserved_name_and_program_state_contracts() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        for (const version of ['webgl','webgl2']) {
            const gl=new OffscreenCanvas(4,4).getContext(version);
            const two=version==='webgl2', prefix=two?'#version 300 es\n':'';
            const program=gl.createProgram();
            for (const [kind,source] of [[gl.VERTEX_SHADER,
                prefix+(two?'in':'attribute')+' vec2 position;void main(){gl_Position=vec4(position,0.,1.);}'],
                [gl.FRAGMENT_SHADER,prefix+'precision highp float;uniform vec4 tint;'+
                (two?'out vec4 color;':'')+'void main(){'+(two?'color':'gl_FragColor')+'=tint;}']]) {
                const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
                if (!gl.getShaderParameter(shader,gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(shader));
                gl.attachShader(program,shader);
            }
            gl.linkProgram(program);
            if (!gl.getProgramParameter(program,gl.LINK_STATUS)) throw Error('link failed');
            const position=gl.getAttribLocation(program,'position');
            const tint=gl.getUniformLocation(program,'tint');
            if (position<0 || !tint) throw Error('real locations absent');
            for (const name of ['gl_VertexID','gl_Position','webgl_private','_webgl_private']) {
                if (gl.getAttribLocation(program,name)!==-1 || gl.getUniformLocation(program,name)!==null)
                    throw Error('reserved location became author storage');
                if (gl.getError()!==0) throw Error('reserved query poisoned error state');
                gl.bindAttribLocation(program,0,name);
                if (gl.getError()!==gl.INVALID_OPERATION) throw Error('reserved bind error');
            }
            for (const name of ['tint@','tint$','tint\\','tint`','tint"',"tint'",'tint\u0000','é']) {
                if (gl.getAttribLocation(program,name)!==-1 || gl.getError()!==gl.INVALID_VALUE)
                    throw Error('attribute character validation');
                if (gl.getUniformLocation(program,name)!==null || gl.getError()!==gl.INVALID_VALUE)
                    throw Error('uniform character validation');
            }
            const unlinked=gl.createProgram();
            if (gl.getAttribLocation(unlinked,'gl_Position')!==-1 || gl.getError()!==gl.INVALID_OPERATION)
                throw Error('reserved attribute skipped linkage validation');
            if (gl.getUniformLocation(unlinked,'gl_Position')!==null || gl.getError()!==gl.INVALID_OPERATION)
                throw Error('reserved uniform skipped linkage validation');
            if (gl.getAttribLocation(program,'position')!==position || gl.getUniformLocation(program,'tint')!==tint)
                throw Error('query errors changed real locations');
            if (gl.getError()!==0) throw Error('query errors remained queued');
        }
    "#,
    );
}
