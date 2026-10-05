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

#[test]
fn webgl_drawing_buffer_dimensions_and_bitmap_transfer_follow_native_admitted_extent() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        for(const api of ['webgl1','webgl2']) {
            const canvas=new OffscreenCanvas(5000,1);
            const gl=api==='webgl2'?__stageWebGl2(canvas,{antialias:false,preserveDrawingBuffer:true}):
                canvas.getContext('webgl',{preserveDrawingBuffer:true});
            if(!gl || canvas.width!==5000 || gl.drawingBufferWidth!==4096 || gl.drawingBufferHeight!==1)
                throw Error('requested canvas extent substituted for native storage');
            if(gl.getParameter(gl.VIEWPORT).join()!=='0,0,4096,1') throw Error('initial native viewport');
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            const bitmap=canvas.transferToImageBitmap();
            if(bitmap.width!==4096 || bitmap.height!==1) throw Error('bitmap extent differs from native snapshot');
            const output=new OffscreenCanvas(1,1).getContext('2d');output.drawImage(bitmap,0,0);
            if(output.getImageData(0,0,1,1).data.join()!=='255,0,0,255') throw Error('reduced buffer pixels');
            const cleared=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,cleared);
            if(cleared.some(value=>value!==0)) throw Error('bitmap transfer did not reset GPU storage');
            canvas.width=6000;
            if(canvas.width!==6000 || gl.drawingBufferWidth!==4096) throw Error('resize content attribute changed');
            gl.viewport(2,3,4,5);canvas.width=2;
            if(gl.drawingBufferWidth!==2 || gl.getParameter(gl.VIEWPORT).join()!=='2,3,4,5')
                throw Error('small resize changed viewport');
            canvas.width=0;canvas.height=0;
            if(gl.drawingBufferWidth!==1 || gl.drawingBufferHeight!==1 || canvas.width!==0 || canvas.height!==0)
                throw Error('zero canvas drawing buffer minimum');
            if(gl.getError()!==0) throw Error('admitted extent generated a GL error');
            gl.getExtension('WEBGL_lose_context').loseContext();
            if(gl.drawingBufferWidth!==0 || gl.drawingBufferHeight!==0) throw Error('lost dimensions');
        }
    "#,
    );
}
