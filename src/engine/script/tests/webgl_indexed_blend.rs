use super::*;

#[test]
fn indexed_blend_idl_native_mrt_pixels_and_atomic_invalid_draws() {
    let source = include_str!("../../../../tests/webgl/indexed-blend.js");
    let (_, outcome) = execute_html(&format!(
        "<script>{source}\ntestIndexedBlend(()=>document.createElement('canvas'));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn indexed_blend_is_not_a_webgl1_extension_or_unadmitted_indexed_query() {
    let (_, outcome) = execute_html(
        r#"<script>
    const one=document.createElement('canvas').getContext('webgl');
    if(one.getExtension('OES_draw_buffers_indexed')!==null||one.getSupportedExtensions().includes('OES_draw_buffers_indexed'))throw Error('WebGL1 exposure');
    const two=document.createElement('canvas').getContext('webgl2');
    if(two.getIndexedParameter(two.COLOR_WRITEMASK,0)!==null||two.getError()!==two.INVALID_ENUM)throw Error('requires admission');
    two.getExtension('OES_draw_buffers_indexed');
    if(Object.prototype.toString.call(two.getExtension('OES_draw_buffers_indexed'))!=='[object OES_draw_buffers_indexed]')throw Error('IDL brand');
    if(two.getIndexedParameter(two.BLEND,0)!==null||two.getError()!==two.INVALID_ENUM)throw Error('no indexed enable getter');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn indexed_blend_stale_extensions_convert_but_cannot_change_restored_state() {
    super::webgl_lifecycle::run(
        r#"
    const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl2');
    const ext=gl.getExtension('OES_draw_buffers_indexed'),loss=gl.getExtension('WEBGL_lose_context');
    canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();setTimeout(()=>loss.restoreContext(),0);});
    canvas.addEventListener('webglcontextrestored',()=>{
        const fresh=gl.getExtension('OES_draw_buffers_indexed');if(!fresh||fresh===ext)throw Error('fresh admission');
        ext.colorMaskiOES(0,false,false,false,false);
        if(gl.getIndexedParameter(gl.COLOR_WRITEMASK,0).join()!=='true,true,true,true')throw Error('stale mask changed restored state');
        fresh.colorMaskiOES(0,true,false,true,false);
        if(gl.getIndexedParameter(gl.COLOR_WRITEMASK,0).join()!=='true,false,true,false'||gl.getError()!==gl.NO_ERROR)throw Error('new mask');
        canvas.dataset.result='pass';
    });
    loss.loseContext();let calls=0;const value={valueOf(){calls++;return 0;}};
    ext.blendFuncSeparateiOES(value,value,value,value,value);if(calls!==5)throw Error('lost conversion');
    "#,
    );
}
