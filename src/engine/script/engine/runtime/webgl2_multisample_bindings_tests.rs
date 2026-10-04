//! Granted realm attributes agree with actual native default-buffer coverage.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_antialias_creation_and_reads_follow_actual_native_samples() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        for(const antialias of [true,false]) {
            const canvas=new OffscreenCanvas(8,8),gl=__stageWebGl2(canvas,{antialias});
            if(!gl || gl.getContextAttributes().antialias!==antialias ||
                gl.getParameter(gl.SAMPLES)!==(antialias?4:0)) throw Error('granted samples');
            const attributes=gl.getContextAttributes();attributes.antialias=!antialias;
            if(gl.getContextAttributes().antialias!==antialias) throw Error('mutable grant');
            const vertex=gl.createShader(gl.VERTEX_SHADER),fragment=gl.createShader(gl.FRAGMENT_SHADER);
            gl.shaderSource(vertex,'#version 300 es\nvoid main(){vec2 p[3]=vec2[3](vec2(-1,-1),vec2(1,-1),vec2(-1,1));gl_Position=vec4(p[gl_VertexID],0,1);}');
            gl.shaderSource(fragment,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}');
            const program=gl.createProgram();
            for(const shader of [vertex,fragment]) {
                gl.compileShader(shader);if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(shader));
                gl.attachShader(program,shader);
            }
            gl.linkProgram(program);gl.useProgram(program);gl.drawArrays(gl.TRIANGLES,0,3);
            const pixels=new Uint8Array(256);gl.readPixels(0,0,8,8,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
            let partial=0;for(let i=0;i<pixels.length;i+=4) if(pixels[i]>0&&pixels[i]<255) partial++;
            if(antialias ? partial===0 : partial!==0) throw Error('reported MSAA has wrong edge coverage');
            if(gl.getError()!==0) throw Error('default read failed');
            canvas.width=4;
            if(gl.drawingBufferWidth!==4 || gl.getParameter(gl.SAMPLES)!==(antialias?4:0)) throw Error('resize sample count');
            const pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
            if(pixel.some(value=>value!==0) || gl.getError()!==0) throw Error('resized bitmap not reset');
        }
        const one=new OffscreenCanvas(2,2).getContext('webgl',{antialias:true});
        if(!one || one.getContextAttributes().antialias || one.getParameter(one.SAMPLES)!==0)
            throw Error('WebGL1 single-sample admission changed');
    "#,
    );
}
