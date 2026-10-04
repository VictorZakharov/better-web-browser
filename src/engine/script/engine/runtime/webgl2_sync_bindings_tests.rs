//! Author fences expose opaque identity and real, task-published completion.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_sync_identity_and_wait_domains_remain_native_owned() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const sync=gl.fenceSync(0x9117,0);
        if (!(sync instanceof __stageWebGl2Types.WebGLSync) || Object.keys(sync).length || !gl.isSync(sync))
            throw Error('opaque fence identity');
        if (gl.getSyncParameter(sync,0x9114)!==0x9118) throw Error('same-task fence publication');
        if (gl.clientWaitSync(sync,1,0)!==0x911b) throw Error('same-task wait blocked or published');
        for (const timeout of [1,2**63,Infinity]) {
            const result=gl.clientWaitSync(sync,0,timeout);
            // Infinity converts to zero under Web IDL, finite nonzero waits reject.
            if (timeout===Infinity ? result!==0x911b : result!==0x911d || gl.getError()!==gl.INVALID_OPERATION)
                throw Error('wait result domain');
        }
        let threw=false; try { gl.clientWaitSync(sync,0,1n); } catch(e) { threw=e instanceof TypeError; }
        if (!threw) throw Error('BigInt timeout accepted');
        if (gl.waitSync(sync,0,-1)!==undefined || gl.getError()!==0) throw Error('signed TIMEOUT_IGNORED');
        const peer=__stageWebGl2(new OffscreenCanvas(2,2));
        if (peer.clientWaitSync(sync,0,0)!==0x911d || peer.getError()!==gl.INVALID_OPERATION) throw Error('foreign fence accepted');
        gl.finish();
        if (gl.getSyncParameter(sync,0x9114)!==0x9118) throw Error('finish published inside task');
    "#,
    );
    context.complete_gpu_task().unwrap();
    check(
        &mut context,
        r#"
        if (gl.getSyncParameter(sync,0x9114)!==0x9119) throw Error('completed native fence not published');
        if (gl.clientWaitSync(sync,0,0)!==0x911a) throw Error('completed fence wait');
        gl.deleteSync(sync); gl.deleteSync(sync);
        if (gl.isSync(sync) || gl.getError()!==0) throw Error('fence deletion not idempotent');
        if (gl.clientWaitSync(sync,0,0)!==0x911d || gl.getError()!==gl.INVALID_OPERATION) throw Error('deleted fence accepted');
    "#,
    );
}

#[test]
fn webgl2_realm_sync_invalid_creation_and_forged_interfaces_fail_before_ipc() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        if (gl.fenceSync(0,0)!==null || gl.getError()!==gl.INVALID_ENUM) throw Error('bad condition');
        if (gl.fenceSync(0x9117,1)!==null || gl.getError()!==gl.INVALID_VALUE) throw Error('bad fence flags');
        for (const name of ['isSync','deleteSync','getSyncParameter','clientWaitSync','waitSync']) {
            let threw=false;
            try { gl[name](Object.create(__stageWebGl2Types.WebGLSync.prototype),0,0); }
            catch(e) { threw=e instanceof TypeError; }
            if (!threw) throw Error('forged fence accepted by '+name);
        }
        if (gl.isSync(null)!==false || gl.deleteSync(null)!==undefined || gl.getError()!==0) throw Error('nullable sync contract');
    "#,
    );
}
