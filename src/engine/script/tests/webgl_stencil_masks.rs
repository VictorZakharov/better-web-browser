//! Unsigned stencil masks must survive signed GLES queries and private clears.
use super::webgl_instancing::check;

#[test]
fn webgl_stencil_masks_preserve_all_unsigned_bits_and_independent_faces() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{stencil:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        const write=()=>[gl.getParameter(gl.STENCIL_WRITEMASK),gl.getParameter(gl.STENCIL_BACK_WRITEMASK)];
        const value=()=>[gl.getParameter(gl.STENCIL_VALUE_MASK),gl.getParameter(gl.STENCIL_BACK_VALUE_MASK)];
        assert(String(write())==='4294967295,4294967295' && String(value())==='4294967295,4294967295','default masks');
        gl.stencilMask(0x80000001);assert(String(write())==='2147483649,2147483649','unsigned write mask');
        gl.stencilMaskSeparate(gl.FRONT,0xffffffff);gl.stencilMaskSeparate(gl.BACK,0x87654321);
        assert(String(write())==='4294967295,2271560481','independent write masks');
        gl.stencilFunc(gl.ALWAYS,0,0x80000001);assert(String(value())==='2147483649,2147483649','unsigned value mask');
        gl.stencilFuncSeparate(gl.FRONT,gl.NEVER,2,0xffffffff);
        gl.stencilFuncSeparate(gl.BACK,gl.ALWAYS,3,0x87654321);
        assert(String(value())==='4294967295,2271560481','independent value masks');
        gl.stencilMaskSeparate(gl.FRONT_AND_BACK,-1);gl.stencilFunc(gl.ALWAYS,0,-1);
        assert(String(write())==='4294967295,4294967295' && String(value())==='4294967295,4294967295','IDL wraps unsigned masks');
        assert(gl.getError()===0,'stencil queries');document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_invalid_stencil_commands_preserve_last_successful_mask_state() {
    check(
        r#"
        const gl=document.querySelector('canvas').getContext('webgl',{stencil:true});
        const assert=(v,s)=>{if(!v)throw Error(s)};
        gl.stencilMaskSeparate(gl.FRONT,0x80000001);gl.stencilMaskSeparate(gl.BACK,0xffffffff);
        gl.stencilFuncSeparate(gl.FRONT,gl.ALWAYS,1,0x87654321);gl.stencilFuncSeparate(gl.BACK,gl.NEVER,2,0xffffffff);
        const snapshot=()=>[gl.STENCIL_WRITEMASK,gl.STENCIL_BACK_WRITEMASK,gl.STENCIL_VALUE_MASK,gl.STENCIL_BACK_VALUE_MASK,
            gl.STENCIL_FUNC,gl.STENCIL_BACK_FUNC,gl.STENCIL_REF,gl.STENCIL_BACK_REF].map(p=>gl.getParameter(p)).join();
        const before=snapshot();
        for(const fail of [()=>gl.stencilMaskSeparate(0xdead,0),()=>gl.stencilFunc(0xdead,0,0),
            ()=>gl.stencilFuncSeparate(gl.FRONT,0xdead,0,0),()=>gl.stencilFuncSeparate(0xdead,gl.ALWAYS,0,0)]) {
            fail();assert(gl.getError()===gl.INVALID_ENUM,'invalid enum');assert(snapshot()===before,'failed state change');
        }
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_private_canvas_reset_preserves_unsigned_stencil_masks() {
    check(
        r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl',{stencil:true});
        gl.stencilMaskSeparate(gl.FRONT,0x80000001);gl.stencilMaskSeparate(gl.BACK,0xffffffff);
        gl.stencilFuncSeparate(gl.FRONT,gl.ALWAYS,1,0x87654321);gl.stencilFuncSeparate(gl.BACK,gl.NEVER,2,0xffffffff);
        canvas.width=9;
        if(gl.drawingBufferWidth!==9 || gl.getParameter(gl.STENCIL_WRITEMASK)!==0x80000001 ||
            gl.getParameter(gl.STENCIL_BACK_WRITEMASK)!==0xffffffff || gl.getParameter(gl.STENCIL_VALUE_MASK)!==0x87654321 ||
            gl.getParameter(gl.STENCIL_BACK_VALUE_MASK)!==0xffffffff || gl.getError()!==0)throw Error('private resize corrupted masks');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_stencil_mask_reflection_resets_on_native_context_restoration() {
    super::webgl_lifecycle::run(
        r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl',{stencil:true});
        gl.stencilMask(0x80000001);gl.stencilFunc(gl.NEVER,2,0x87654321);
        const loss=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();setTimeout(()=>loss.restoreContext(),0)});
        canvas.addEventListener('webglcontextrestored',()=>{
            for(const p of [gl.STENCIL_WRITEMASK,gl.STENCIL_BACK_WRITEMASK,gl.STENCIL_VALUE_MASK,gl.STENCIL_BACK_VALUE_MASK])
                if(gl.getParameter(p)!==0xffffffff)throw Error('old mask survived restoration');
            if(gl.getParameter(gl.STENCIL_FUNC)!==gl.ALWAYS || gl.getParameter(gl.STENCIL_REF)!==0)throw Error('old stencil function');
            canvas.dataset.result='pass';
        });
        loss.loseContext();
    "#,
    );
}
