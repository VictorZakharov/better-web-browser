//! Drawing-buffer extent queries need resize synchronization, not a pixel export.
use super::webgl_owned_copy_tests::both;
use super::webgl2_bindings_tests::{check, document};

#[test]
fn extent_queries_do_not_read_back_a_dirty_surface() {
    let (mut context, host) = document();
    check(
        &mut context,
        r#"
        const canvas=new OffscreenCanvas(8,4),gl=canvas.getContext('webgl2');
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#,
    );
    host.borrow_mut().host_call_profile.set_enabled(true);
    check(
        &mut context,
        r#"
        for(let i=0;i<3;i++)if(gl.drawingBufferWidth!==8||gl.drawingBufferHeight!==4)throw Error('wrong extent');
        canvas.width=6;canvas.height=3;
        if(gl.drawingBufferWidth!==6||gl.drawingBufferHeight!==3)throw Error('resize not applied');
    "#,
    );
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglSnapshot"),
        0
    );
}

#[test]
fn dimensions_resize_lost_contexts_and_bitmap_exports_keep_public_behavior_in_both_realms() {
    both(
        r#"
        const canvas=new OffscreenCanvas(2,2),gl=canvas.getContext('webgl2',{preserveDrawingBuffer:true});
        const assert=(ok,message)=>{if(!ok)throw Error(message)};
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        assert(gl.drawingBufferWidth===2&&gl.drawingBufferHeight===2,'initial dimensions');
        const target=new OffscreenCanvas(2,2).getContext('2d');target.drawImage(canvas,0,0);
        assert(String(target.getImageData(0,0,1,1).data)==='255,0,0,255','dimension query consumed paint');
        canvas.width=3;canvas.height=1;
        assert(gl.drawingBufferWidth===3&&gl.drawingBufferHeight===1,'resize dimensions');
        const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,0,0,0','resize must clear buffer');
        const extension=gl.getExtension('WEBGL_lose_context');extension.loseContext();
        assert(gl.drawingBufferWidth===0&&gl.drawingBufferHeight===0,'lost dimensions');
    "#,
    );
}
