//! Native Canvas readback retains alpha semantics and independent public data.
use super::webgl_owned_copy_tests::both;

#[test]
fn native_canvas_consumers_receive_correct_alpha_and_independent_successive_frames() {
    both(
        r#"
        for(const api of ['webgl','webgl2'])for(const premultipliedAlpha of [true,false]){
            const source=new OffscreenCanvas(2,2);
            const gl=source.getContext(api,{alpha:true,premultipliedAlpha,preserveDrawingBuffer:true,antialias:false});
            if(!gl)throw Error('native context '+api);
            const target=new OffscreenCanvas(2,2).getContext('2d');
            gl.clearColor(.25,.125,0,.5);gl.clear(gl.COLOR_BUFFER_BIT);
            target.drawImage(source,0,0);
            const first=target.getImageData(0,0,1,1);
            const expected=premultipliedAlpha?[128,64,0,128]:[64,32,0,128];
            for(let i=0;i<4;i++)if(Math.abs(first.data[i]-expected[i])>1)throw Error('alpha '+api+' '+first.data);
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            target.clearRect(0,0,2,2);target.drawImage(source,0,0);
            if(String(target.getImageData(0,0,1,1).data)!=='0,255,0,255')throw Error('new frame');
            for(let i=0;i<4;i++)if(Math.abs(first.data[i]-expected[i])>1)throw Error('previous public readback aliased new frame');
            if(gl.getError()!==0)throw Error('readback introduced native error');
            // Retire each fully checked variant rather than retaining all
            // Window and Worker variants against the renderer-wide cap.
            gl.getExtension('WEBGL_lose_context').loseContext();
        }
    "#,
    );
}

#[test]
fn native_bitmap_consumers_keep_raw_alpha_pack_state_and_context_attributes() {
    both(
        r#"
        for(const api of ['webgl','webgl2']) for(const alpha of [true,false])
            for(const premultipliedAlpha of [true,false]) {
                const source=new OffscreenCanvas(3,2);
                const gl=source.getContext(api,{alpha,premultipliedAlpha,preserveDrawingBuffer:true,antialias:false});
                const target=new OffscreenCanvas(3,2).getContext('2d');
                const raw=new Uint8Array(8),sentinel=new Uint8Array([79,79,79,79]);
                gl.pixelStorei(gl.PACK_ALIGNMENT,8);
                gl.clearColor(.25,.125,0,.5);gl.clear(gl.COLOR_BUFFER_BIT);
                gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,raw);
                target.drawImage(source,0,0);
                const copy=target.getImageData(0,0,1,1).data;
                const expected=!alpha?[64,32,0,255]:premultipliedAlpha?[128,64,0,128]:[64,32,0,128];
                for(let i=0;i<4;i++)if(Math.abs(copy[i]-expected[i])>1)throw Error('bitmap alpha '+copy);
                if(gl.getParameter(gl.PACK_ALIGNMENT)!==8)throw Error('snapshot modified pack state');
                gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,sentinel);
                for(let i=0;i<4;i++)if(sentinel[i]!==raw[i])throw Error('Canvas mutated drawing buffer');
                // Raw GL invalid-state errors survive conversion/bitmap readback.
                gl.enable(0xffff);target.drawImage(source,0,0);
                if(gl.getError()!==gl.INVALID_ENUM || gl.getError()!==0)throw Error('snapshot consumed GL error');
                gl.clearColor(1,.5,.25,0);gl.clear(gl.COLOR_BUFFER_BIT);
                target.clearRect(0,0,3,2);target.drawImage(source,0,0);
                if(String(target.getImageData(0,0,1,1).data)!=='0,0,0,0' && alpha)
                    throw Error('transparent copied bitmap retained hidden RGB');
                gl.getExtension('WEBGL_lose_context').loseContext();
            }
    "#,
    );
}
