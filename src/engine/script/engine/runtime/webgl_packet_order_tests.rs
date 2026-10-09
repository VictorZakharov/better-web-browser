//! Public commands retain their per-call values and all observation barriers.
use super::webgl_numeric_command_tests::PROGRAM;
use super::webgl_owned_copy_tests::both;
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl_packet_coalesces_callbacks_without_eliminating_setters() {
    let (mut context, host) = document();
    check(&mut context, PROGRAM);
    host.borrow_mut().host_call_profile.set_enabled(true);
    check(
        &mut context,
        r#"
        for(let index=0;index<1000;index++)gl.clearColor((index&127)/128,1,0,1);
        if(gl.getError()!==0)throw Error('native errors');
        if(String(gl.getParameter(gl.COLOR_CLEAR_VALUE))!=='0.8046875,1,0,1')throw Error('last setter');
    "#,
    );
    // Seven full groups and one observation-drained tail, not 1,000 callbacks.
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandPacket"),
        8
    );
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandValues"),
        0
    );
}

#[test]
fn webgl_packet_tail_submits_at_a_microtask_checkpoint_not_a_gpu_publication() {
    let (mut context, host) = document();
    check(&mut context, PROGRAM);
    // Initialization's useProgram was drained by uniform-location observations.
    host.borrow_mut().host_call_profile.set_enabled(true);
    check(
        &mut context,
        "gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);",
    );
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandPacket"),
        0
    );
    context.run_jobs().unwrap();
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandPacket"),
        1
    );
    context.complete_gpu_task().unwrap();
    check(
        &mut context,
        r#"
        const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,255,0,255','task-tail native pixel');
    "#,
    );
}

#[test]
fn webgl_packet_microtask_uses_captured_intrinsics_despite_author_promise_hooks() {
    let (mut context, host) = document();
    check(&mut context, PROGRAM);
    host.borrow_mut().host_call_profile.set_enabled(true);
    check(
        &mut context,
        r#"
        let packetHooks=0;
        const intrinsicPromise=Promise,oldThen=Promise.prototype.then;
        const constructor=Object.getOwnPropertyDescriptor(Promise.prototype,'constructor');
        const species=Object.getOwnPropertyDescriptor(Promise,Symbol.species);
        const fail=()=>{packetHooks++;throw Error('author promise hook');};
        Promise.prototype.then=fail;
        Object.defineProperty(Promise.prototype,'constructor',{configurable:true,get:fail});
        Object.defineProperty(Promise,Symbol.species,{configurable:true,get:fail});
        globalThis.Promise=fail;globalThis.Float64Array=fail;
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
    "#,
    );
    context.run_jobs().unwrap();
    assert_eq!(
        host.borrow()
            .host_call_profile
            .calls_for_test("webglCommandPacket"),
        1
    );
    context.complete_gpu_task().unwrap();
    check(
        &mut context,
        r#"
        globalThis.Promise=intrinsicPromise;
        Promise.prototype.then=oldThen;
        Object.defineProperty(Promise.prototype,'constructor',constructor);
        Object.defineProperty(Promise,Symbol.species,species);
        assert(packetHooks===0,'packet scheduling did not invoke author hooks');
        const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,255,0,255','captured scheduling submitted real pixels');
    "#,
    );
}

#[test]
fn webgl_packet_webgl1_observations_and_native_draws_keep_setter_order() {
    both(
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl',{antialias:false,preserveDrawingBuffer:true});
        for(let index=0;index<257;index++)gl.clearColor(index===256?0:1,index===256?1:0,0,1);
        gl.clear(gl.COLOR_BUFFER_BIT);
        if(String(gl.getParameter(gl.COLOR_CLEAR_VALUE))!=='0,1,0,1')throw Error('WebGL 1 observation');
        const pixels=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(String(pixels)!=='0,255,0,255'||gl.getError())throw Error('WebGL 1 native pixel');
        gl.getExtension('WEBGL_lose_context').loseContext();
    "#,
    );
}

#[test]
fn webgl_packet_retains_each_reused_uniform_value_across_draws_and_capacity_flushes() {
    both(&format!(
        r#"{PROGRAM}
        const source=new Float32Array([1,0,0,1]),values=new Float32Array([0,0,0,0,0,0]);
        gl.uniformMatrix2x3fv(matrix,false,values);
        for(let index=0;index<130;index++) {{
            const green=index===129;
            source.set([green?0:1,green?1:0,0,1]);
            gl.uniform4fv(color,source);gl.drawArrays(gl.POINTS,0,1);
            source.fill(17);values.fill(19);
        }}
        const pixels=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,255,0,255','last converted value and draw order');
        assert(String(gl.getUniform(p,color))==='0,1,0,1','buffer reuse does not mutate queued upload');
        assert(gl.getError()===0,'capacity-flush errors');
    "#
    ));
}

#[test]
fn webgl_packet_reentrant_converters_observe_preceding_commands_not_later_ones() {
    both(&format!(
        r#"{PROGRAM}
        gl.clearColor(.25,.5,.75,1);
        const order=[];
        gl.clearColor({{valueOf(){{
            order.push(String(gl.getParameter(gl.COLOR_CLEAR_VALUE)));return 1;
        }}}},0,0,1);
        assert(String(order)==='0.25,0.5,0.75,1','reentrant observation barrier');
        assert(String(gl.getParameter(gl.COLOR_CLEAR_VALUE))==='1,0,0,1','later state setter');
        gl.enable(0xdead);gl.viewport(0,0,-1,2);gl.clearColor(0,1,0,1);
        assert(gl.getError()===gl.INVALID_ENUM,'first deferred error');
        assert(gl.getError()===gl.INVALID_VALUE,'second deferred error');
        assert(gl.getError()===0,'error stream drained');
    "#
    ));
}

#[test]
fn webgl_packet_cross_context_observations_and_uploads_preserve_one_realm_order() {
    both(
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        const peer=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        for(let index=0;index<129;index++){
            gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            peer.clearColor(1,0,0,1);peer.clear(peer.COLOR_BUFFER_BIT);
        }
        const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([1,2,3,4]),gl.STATIC_DRAW);
        const bytes=new Uint8Array(4);gl.getBufferSubData(gl.ARRAY_BUFFER,0,bytes);
        if(String(bytes)!=='1,2,3,4')throw Error('binding/owned upload barrier');
        const pixels=new Uint8Array(4);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(String(pixels)!=='0,255,0,255')throw Error('first context order');
        peer.readPixels(0,0,1,1,peer.RGBA,peer.UNSIGNED_BYTE,pixels);
        if(String(pixels)!=='255,0,0,255'||gl.getError()||peer.getError())throw Error('peer context order');
        gl.getExtension('WEBGL_lose_context').loseContext();
        peer.getExtension('WEBGL_lose_context').loseContext();
    "#,
    );
}

#[test]
fn webgl_packet_all_observations_drain_large_list_fallbacks_in_the_same_order() {
    both(&format!(
        r#"{PROGRAM}
        gl.uniform4fv(color,new Float32Array([.25,.5,.75,1]));
        const large=new Float32Array(68);large.fill(1);
        gl.uniform4fv(color,large); // Native INVALID_OPERATION: uniform is not an array.
        gl.uniform4fv(color,new Float32Array([0,1,0,1]));
        assert(gl.getError()===gl.INVALID_OPERATION,'large fallback keeps its error position');
        assert(String(gl.getUniform(p,color))==='0,1,0,1','setter after fallback');
        assert(gl.getError()===0,'no spurious packet error');
    "#
    ));
}

#[test]
fn webgl_packet_simulated_loss_drains_then_retires_names_before_a_new_context() {
    both(
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false});
        gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.getExtension('WEBGL_lose_context').loseContext();
        if(!gl.isContextLost()||gl.getError()!==gl.CONTEXT_LOST_WEBGL||gl.getError()!==0)
            throw Error('loss must be reported once');
        const next=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
        next.clearColor(1,0,0,1);next.clear(next.COLOR_BUFFER_BIT);
        const pixels=new Uint8Array(4);next.readPixels(0,0,1,1,next.RGBA,next.UNSIGNED_BYTE,pixels);
        if(String(pixels)!=='255,0,0,255'||next.getError())throw Error('retired packet aliased new context');
        next.getExtension('WEBGL_lose_context').loseContext();
    "#,
    );
}
