use super::*;

struct Realm {
    // Persistent handles must disappear before Agent releases their isolate.
    context: v8::Global<v8::Context>,
    agent: Agent,
}

impl Realm {
    fn new() -> Self {
        crate::engine::script::engine::runtime::initialize_v8();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        let mut agent = Agent::new(isolate).unwrap();
        let context = agent
            .run(|isolate| {
                v8::scope!(let scope, isolate);
                let context = v8::Context::new(scope, Default::default());
                Ok(v8::Global::new(scope, context))
            })
            .unwrap();
        Self { context, agent }
    }
    fn eval(&mut self, source: &str) -> String {
        let context = self.context.clone();
        self.agent
            .run(|isolate| {
                v8::scope!(let scope,isolate);
                let local = v8::Local::new(scope, context);
                let scope = &mut v8::ContextScope::new(scope, local);
                let source = v8::String::new(scope, source).unwrap();
                let script = v8::Script::compile(scope, source, None).unwrap();
                Ok(script.run(scope).unwrap().to_rust_string_lossy(scope))
            })
            .unwrap()
    }
    fn hint(&mut self, epoch: u64, level: Level) {
        self.agent
            .deliver_memory_pressure(Advice {
                epoch,
                level,
                private: 900 * 1024 * 1024,
            })
            .unwrap();
    }
}

#[test]
fn pressure_keeps_live_backing_stores_and_pending_jobs_intact() {
    let mut realm = Realm::new();
    realm.agent.set_gc_profiling(true);
    realm.eval("var bytes=new Uint8Array(12*1024*1024);bytes[0]=17;bytes[bytes.length-1]=93;var jobs=[];Promise.resolve().then(()=>jobs.push('job'))");
    realm.hint(1, Level::Critical);
    assert!(
        realm.agent.gc_sample().0 > 0,
        "the native pressure hint must reach V8"
    );
    assert_eq!(
        realm.eval("[bytes.length,bytes[0],bytes[bytes.length-1],jobs.length].join()"),
        "12582912,17,93,0",
        "policy must not detach buffers or dispatch jobs"
    );
    realm
        .agent
        .run(|isolate| {
            isolate.perform_microtask_checkpoint();
            Ok(())
        })
        .unwrap();
    assert_eq!(realm.eval("jobs.join()"), "job");
    let reports = realm.agent.take_pressure_diagnostics();
    assert!(reports.iter().any(|row| row.contains("critical=1")));
    assert!(realm.agent.take_pressure_diagnostics().is_empty());
}

#[test]
fn hints_do_not_clear_weakref_kept_objects_inside_a_task() {
    let mut realm = Realm::new();
    realm.eval("var weak;(()=>{const value={token:42};weak=new WeakRef(value)})();");
    realm.hint(1, Level::Critical);
    assert_eq!(realm.eval("weak.deref().token"), "42");
    realm.hint(2, Level::Moderate);
    assert_eq!(realm.eval("weak.deref().token"), "42");
}

#[test]
fn duplicate_epoch_does_not_collect_again_and_cancelled_agent_cannot_acknowledge_it() {
    let mut realm = Realm::new();
    realm.agent.set_gc_profiling(true);
    realm.hint(1, Level::Critical);
    let collected = realm.agent.gc_sample().0;
    realm.hint(1, Level::Critical);
    assert_eq!(realm.agent.gc_sample().0, collected);
    realm.agent.cancellation().cancel();
    let advice = Advice {
        epoch: 2,
        level: Level::Critical,
        private: 900 * 1024 * 1024,
    };
    assert!(
        realm
            .agent
            .deliver_memory_pressure(advice)
            .unwrap_err()
            .message
            .contains("cancelled")
    );
    assert!(realm.agent.pressure.pending(advice));
    assert_eq!(realm.agent.gc_sample().0, collected);
}

#[test]
fn recovery_and_cold_pressure_hints_are_not_author_profiling_switches() {
    let mut realm = Realm::new();
    realm.hint(1, Level::Moderate);
    realm.hint(2, Level::Critical);
    realm.hint(3, Level::None);
    assert!(realm.agent.take_task_diagnostics().is_empty());
    assert_eq!(realm.eval("21*2"), "42");
    // Each hint balances the isolate entry guard; a separate agent still works.
    let mut peer = Realm::new();
    peer.hint(1, Level::Critical);
    assert_eq!(peer.eval("7*6"), "42");
    assert_eq!(realm.eval("6*7"), "42");
}
