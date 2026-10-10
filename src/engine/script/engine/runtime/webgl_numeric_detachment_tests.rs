//! Detachment admission reads native slots without an allocation per uniform.
use super::webgl_numeric_command_tests::PROGRAM;
use super::webgl_owned_copy_tests::both;
use super::webgl2_bindings_tests::{check, staged_bootstrap};
use super::*;
use crate::engine::script::{HostState, bootstrap, module_loader::WebModuleLoader};

#[test]
fn typed_uniform_detachment_checks_do_not_allocate_temporary_views() {
    for fallback in [false, true] {
        let host = Rc::new(RefCell::new(HostState::new(
            crate::engine::dom::parse("<!doctype html><body>").document,
            "https://example.test/",
            "UTF-8",
            Rc::new(WebModuleLoader::new()),
        )));
        let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
        check(
            &mut context,
            r#"
            if(typeof Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'detached')?.get!=='function')
                throw Error('pinned engine lacks intrinsic detachment admission');
            let temporaryViews=0;
            globalThis.Uint8Array=new Proxy(Uint8Array,{construct(target,args){
                if(args.length===3&&args[2]===0)temporaryViews++;
                return Reflect.construct(target,args);
            }});
        "#,
        );
        let source = staged_bootstrap(bootstrap::BROWSER_BOOTSTRAP);
        let source = if fallback {
            let captured = "const webGlNumericDetachedGetter = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'detached')?.get;";
            assert!(source.contains(captured));
            source.replace(captured, "const webGlNumericDetachedGetter = undefined;")
        } else {
            source
        };
        check(&mut context, &source);
        check(
            &mut context,
            &format!(
                r#"{PROGRAM}
            temporaryViews=0;
            const data=new Float32Array([.25,.5,.75,1]);
            for(let i=0;i<3;i++)gl.uniform4fv(color,data);
            assert(temporaryViews==={expected},'temporary view admission count');
            assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','actual native uniform');
            structuredClone(data.buffer,{{transfer:[data.buffer]}});
            gl.uniform4fv(color,data);
            assert(gl.getError()===gl.INVALID_VALUE,'detached source error');
            assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','detached source did not overwrite');
            void 0;
        "#,
                expected = if fallback { 3 } else { 0 }
            ),
        );
    }
}

#[test]
fn detachment_admission_ignores_author_getters_and_keeps_shared_snapshots() {
    both(&format!(
        r#"{PROGRAM}
        const data=new Float32Array([.25,.5,.75,1]);
        const descriptor=Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'detached');
        const fail=()=>{{throw Error('author detached getter')}};
        Object.defineProperty(ArrayBuffer.prototype,'detached',{{configurable:true,get:fail}});
        Object.defineProperty(data.buffer,'detached',{{get:fail}});
        try {{
            gl.uniform4fv(color,data);
            assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','intrinsic getter');
            const shared=new Float32Array(new SharedArrayBuffer(16));shared.set([.5,.25,.75,1]);
            gl.uniform4fv(color,shared);shared.fill(99);
            assert(String(gl.getUniform(p,color))==='0.5,0.25,0.75,1','independent shared snapshot');
            gl.uniform4fv(color,new Float32Array(0));
            assert(gl.getError()===gl.INVALID_VALUE,'zero length is not a detached-buffer exception');
            assert(gl.getError()===0,'no additional errors');
        }} finally {{Object.defineProperty(ArrayBuffer.prototype,'detached',descriptor)}}
        void 0;
    "#
    ));
}
