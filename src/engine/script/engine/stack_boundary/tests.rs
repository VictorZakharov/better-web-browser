//! Real V8 recursion must remain a catchable RangeError on smaller owner stacks.
use super::super::{bridge::HostBridge, runtime::Context, value::Source};

#[test]
fn recursion_is_catchable_and_isolate_remains_usable_on_actual_small_windows_stacks() {
    for size in [512 * 1024, 1024 * 1024, 2 * 1024 * 1024] {
        std::thread::Builder::new()
            .name("v8-stack-boundary".into())
            .stack_size(size)
            .spawn(|| {
                let mut context = Context::new(HostBridge::Worker(std::rc::Weak::new())).unwrap();
                let value = context.eval(Source::from_bytes(r#"
                let depth=0;
                function recurse(){depth++;return recurse()+1;}
                let failure='none';try{recurse();}catch(e){failure=e.name;}
                if(depth<16||failure!=='RangeError')throw Error('stack limit did not surface as RangeError');
                failure
                "#)).unwrap();
                assert_eq!(value.string_value(), "RangeError");
                assert_eq!(context.eval(Source::from_bytes("6*7")).unwrap().string_value(), "42");
                context.eval(Source::from_bytes("globalThis.checkpoint=0;Promise.resolve().then(()=>checkpoint=71);")).unwrap();
                context.run_jobs().unwrap();
                assert_eq!(context.eval(Source::from_bytes("checkpoint")).unwrap().string_value(), "71");
                assert!(!context.cancellation().is_cancelled());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[test]
fn wasm_recursion_checks_the_same_small_owner_stack_and_survives_reentry() {
    std::thread::Builder::new()
        .name("v8-wasm-stack-boundary".into())
        .stack_size(512 * 1024)
        .spawn(|| {
            use crate::engine::script::{ScriptKind, worker_host::WorkerHostState};
            let host = std::rc::Rc::new(std::cell::RefCell::new(WorkerHostState::new(
                "https://example.com/worker.js", true, "", ScriptKind::Classic,
                std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
                std::sync::Arc::new(crate::fetch::csp::PolicyContainer::default()),
            )));
            let mut context = Context::new(HostBridge::Worker(std::rc::Rc::downgrade(&host))).unwrap();
            context.eval(Source::from_bytes(r#"
                // One () -> () function, exported as f, whose body is call 0.
                // No imported host function, memory, or dependency is involved.
                const module=new WebAssembly.Module(new Uint8Array([
                    0,97,115,109,1,0,0,0,
                    1,4,1,96,0,0, 3,2,1,0, 7,5,1,1,102,0,0,
                    10,6,1,4,0,16,0,11
                ]));
                globalThis.wasmRecurse=new WebAssembly.Instance(module).exports.f;
            "#)).unwrap();
            for _ in 0..3 {
                let value = context.eval(Source::from_bytes(
                    "(()=>{let caught=false;try{wasmRecurse();}catch(e){caught=e instanceof RangeError;}return caught;})()"
                )).unwrap();
                assert_eq!(value.string_value(), "true");
                assert_eq!(context.eval(Source::from_bytes("40+2")).unwrap().string_value(), "42");
                assert!(!context.cancellation().is_cancelled());
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn insufficient_stack_is_rejected_before_creating_an_isolate_or_arming_a_timer() {
    std::thread::Builder::new()
        .name("v8-insufficient-stack".into())
        .stack_size(256 * 1024)
        .spawn(|| {
            let failure = super::Boundary::new()
                .err()
                .expect("native reserve must not be spent on JS");
            assert!(
                failure
                    .message
                    .contains("insufficient or unrecognized stack space")
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
