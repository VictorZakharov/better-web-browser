//! Independent bridge allocations are movable; public BufferSource semantics are not.
use super::webgl2_bindings_tests::{check, document, staged_bootstrap};
use super::*;
use crate::engine::script::{ScriptKind, worker_host::WorkerHostState};
use std::sync::Arc;

pub(super) fn both(code: &str) {
    let (mut window, _document) = document();
    check(&mut window, code);
    let host = Rc::new(RefCell::new(WorkerHostState::new(
        "https://example.test/worker.js",
        true,
        "",
        ScriptKind::Classic,
        Arc::new(|url, _| Err(format!("unexpected {url}"))),
        Arc::new(crate::fetch::csp::PolicyContainer::default()),
    )));
    let mut worker = Context::new(HostBridge::Worker(Rc::downgrade(&host))).unwrap();
    check(
        &mut worker,
        &staged_bootstrap(crate::engine::script::worker_bootstrap::WORKER_BOOTSTRAP),
    );
    check(&mut worker, code);
}

pub(super) const SETUP: &str = r#"
    const gl=new OffscreenCanvas(4,4).getContext('webgl2',{antialias:false});
    if(!gl)throw Error('actual WebGL2 context unavailable');
    const expect=(actual,expected,label)=>{if(String(actual)!==String(expected))
        throw Error(label+': '+actual+' expected '+expected);};
    const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
    const read=size=>{const out=new Uint8Array(size);gl.getBufferSubData(gl.ARRAY_BUFFER,0,out);return out;};
"#;

#[test]
fn webgl_owned_upload_keeps_copy_semantics_after_source_mutation_and_transfer() {
    both(&format!(
        r#"{SETUP}
        const backing=new ArrayBuffer(32),bytes=new Uint8Array(backing);bytes.fill(99);
        const span=new Uint8Array(backing,8,8);span.set([1,2,3,4,5,6,7,8]);
        gl.bufferData(gl.ARRAY_BUFFER,span,gl.DYNAMIC_DRAW);
        expect(span,[1,2,3,4,5,6,7,8],'upload must not mutate/detach its source');
        span.fill(17);expect(read(8),[1,2,3,4,5,6,7,8],'post-upload mutation');
        const transferred=structuredClone(backing,{{transfer:[backing]}});
        if(backing.byteLength!==0 || transferred.byteLength!==32)throw Error('explicit author transfer');
        expect(read(8),[1,2,3,4,5,6,7,8],'detachment after native copy');
        if(gl.getError()!==0)throw Error('owned upload native error');
    "#
    ));
}

#[test]
fn webgl_owned_subupload_overloads_use_element_ranges_and_leave_neighbors_intact() {
    both(&format!(
        r#"{SETUP}
        gl.bufferData(gl.ARRAY_BUFFER,16,gl.DYNAMIC_DRAW);
        const whole=new Uint16Array([99,0x1234,0xabcd,0x4321,99]),span=whole.subarray(1,4);
        gl.bufferSubData(gl.ARRAY_BUFFER,4,span,1,2);
        const expected=new Uint8Array(16),selected=new Uint8Array(whole.buffer,4,4);
        expected.set(selected,4);span.fill(17);
        expect(read(16),expected,'element offsets select actual bytes');
        const destination=new Uint8Array(24);destination.fill(83);
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,destination.subarray(4,20));
        expect(destination.subarray(0,4),[83,83,83,83],'read prefix');
        expect(destination.subarray(4,20),expected,'read body');
        expect(destination.subarray(20),[83,83,83,83],'read suffix');
        if(gl.getError()!==0)throw Error('owned subupload native error');
    "#
    ));
}

#[test]
fn webgl_owned_texture_copy_cannot_alias_a_mutated_author_view() {
    both(&format!(
        r#"{SETUP}
        const texture=gl.createTexture(),framebuffer=gl.createFramebuffer();
        gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
        const backing=new Uint8Array(16);backing.fill(99);
        const source=backing.subarray(4,8);source.set([12,34,56,255]);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        source.fill(77);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const destination=new Uint8Array(12);destination.fill(83);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,destination.subarray(4,8));
        expect(destination,[83,83,83,83,12,34,56,255,83,83,83,83],'native copied texture and destination bounds');
        expect(backing.subarray(0,4),[99,99,99,99],'source prefix');
        expect(backing.subarray(8),[99,99,99,99,99,99,99,99],'source suffix');
        if(gl.getError()!==0)throw Error('owned texture native error');
    "#
    ));
}

#[test]
fn webgl_owned_readback_preserves_pack_padding_and_destination_offsets_in_both_realms() {
    both(&format!(
        r#"{SETUP}
        gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.pixelStorei(gl.PACK_ALIGNMENT,8);gl.pixelStorei(gl.PACK_ROW_LENGTH,4);
        gl.pixelStorei(gl.PACK_SKIP_ROWS,1);gl.pixelStorei(gl.PACK_SKIP_PIXELS,1);
        const output=new Uint8Array(72);output.fill(83);
        gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,output.subarray(4,68),4);
        for(let i=0;i<output.length;i++){{
            const inRow=(i>=28&&i<36)||(i>=44&&i<52);
            const wanted=inRow?([255,0,0,255][i%4]):83;
            if(output[i]!==wanted)throw Error('pack padding byte '+i+': '+output[i]);
        }}
        if(gl.getError()!==0)throw Error('owned packed readback native error');
    "#
    ));
}

#[test]
fn webgl_owned_failed_native_readback_never_mutates_author_destination() {
    both(&format!(
        r#"{SETUP}
        const output=new Uint8Array(16);output.fill(83);
        gl.readPixels(0,0,4,4,gl.RGBA,gl.UNSIGNED_BYTE,output);
        if(gl.getError()!==gl.INVALID_OPERATION)throw Error('undersized destination accepted');
        expect(output,new Uint8Array(16).fill(83),'failed read remains atomic');
        const target=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,target);
        gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,output);
        if(gl.getError()!==gl.INVALID_FRAMEBUFFER_OPERATION)throw Error('incomplete framebuffer accepted');
        expect(output,new Uint8Array(16).fill(83),'native rejection does not publish a reply');
    "#
    ));
}
