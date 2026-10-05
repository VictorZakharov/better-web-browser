//! Real default-surface clears and independent read/draw routes through Web IDL.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_core_clears_schedule_connected_canvas_presentation() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const canvas=document.createElement('canvas'); canvas.width=2; canvas.height=2;
        document.body.appendChild(canvas);
        const gl=__stageWebGl2(canvas,{preserveDrawingBuffer:true});
        gl.clearBufferfv(0x1800,0,[1,0,0,1]);
        const first=__takeCanvasPresentation();
        if (first.length!==1 || [...first[0][5].slice(0,4)].join()!=='255,0,0,255') throw Error('typed clear not presented');
        if (__takeCanvasPresentation().length!==0) throw Error('idle canvas exported twice');
        gl.clearColor(0,1,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        const second=__takeCanvasPresentation();
        if (second.length!==1 || [...second[0][5].slice(0,4)].join()!=='0,255,0,255') throw Error('shared clear not presented');
        if (__takeCanvasPresentation().length!==0) throw Error('second idle canvas exported twice');
    "#,
    );
}

#[test]
fn webgl2_realm_typed_clear_offsets_and_invalidation_preserve_native_pixels() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4),{preserveDrawingBuffer:true});
        gl.clearBufferfv(0x1800,0,new Float32Array([99,0,1,0,1,98]),1);
        const pixel=new Uint8Array(4);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,255,0,255') throw Error('offset float clear');
        gl.invalidateFramebuffer(gl.FRAMEBUFFER,[0x1800]);
        gl.invalidateSubFramebuffer(gl.FRAMEBUFFER,[0x1800],-1,-1,3,3);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,255,0,255' || gl.getError()!==0) throw Error('undefined invalidated pixels');
        gl.clearBufferfv(0x1800,0,[1,0,0],0);
        if (gl.getError()!==gl.INVALID_VALUE) throw Error('short clear list accepted');
        gl.clearBufferfv(0x1800,0,[1,0,0,1],1);
        if (gl.getError()!==gl.INVALID_VALUE) throw Error('short offset clear accepted');
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
        if ([...pixel].join()!=='0,255,0,255') throw Error('failed clear mutated storage');
    "#,
    );
}

#[test]
fn webgl2_realm_sample_queries_and_read_routes_return_core_types() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(4,4));
        const samples=gl.getInternalformatParameter(gl.RENDERBUFFER,0x8058,0x80a9);
        if (!(samples instanceof Int32Array) || !samples.length || samples.some(n=>n<0)) throw Error('sample query domain');
        if (gl.getInternalformatParameter(gl.TEXTURE_2D,0x8058,0x80a9)!==null || gl.getError()!==gl.INVALID_ENUM) throw Error('bad query target');
        const read=gl.createFramebuffer(), draw=gl.createFramebuffer();
        gl.bindFramebuffer(0x8ca8,read); gl.bindFramebuffer(0x8ca9,draw);
        if (gl.getParameter(0x8caa)!==read || gl.getParameter(gl.FRAMEBUFFER_BINDING)!==draw) throw Error('read/draw identity');
        gl.readBuffer(gl.NONE); gl.drawBuffers([gl.NONE]);
        if (gl.getParameter(0x0c02)!==gl.NONE || gl.getParameter(0x8825)!==gl.NONE) throw Error('read/draw routes');
        gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.readBuffer(gl.BACK); gl.drawBuffers([gl.BACK]);
        if (gl.getParameter(0x0c02)!==gl.BACK || gl.getError()!==0) throw Error('default route');
    "#,
    );
}
