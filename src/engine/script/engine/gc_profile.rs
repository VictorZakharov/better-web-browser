//! Opt-in owner-thread GC attribution, without inspecting author objects.
//! V8's public prologue/epilogue callbacks bracket collection work; they do not
//! account for concurrent background GC. No allocation or JS occurs in callbacks.
//! https://v8.github.io/api/head/classv8_1_1Isolate.html
use std::cell::Cell;
use std::ffi::c_void;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Profile {
    // The box's address stays fixed when its Agent moves. Only the isolate's
    // owning thread accesses these cells, including V8's callbacks.
    state: Option<Box<State>>,
}

#[derive(Default)]
struct State {
    started: Cell<Option<Instant>>,
    count: Cell<u64>,
    elapsed: Cell<Duration>,
}

impl Profile {
    pub(super) fn enable(&mut self, isolate: &mut v8::OwnedIsolate, enabled: bool) {
        if !enabled {
            self.detach(isolate);
        } else if self.state.is_none() {
            let mut state = Box::<State>::default();
            let data = std::ptr::from_mut(state.as_mut()).cast();
            isolate.add_gc_prologue_callback(begin, data, v8::GCType::kGCTypeAll);
            isolate.add_gc_epilogue_callback(end, data, v8::GCType::kGCTypeAll);
            self.state = Some(state);
        }
    }

    pub(super) fn detach(&mut self, isolate: &mut v8::OwnedIsolate) {
        if let Some(mut state) = self.state.take() {
            let data = std::ptr::from_mut(state.as_mut()).cast();
            // Remove both callbacks before freeing their data, while the
            // isolate is entered and still alive. Never leave a dangling hook.
            isolate.remove_gc_prologue_callback(begin, data);
            isolate.remove_gc_epilogue_callback(end, data);
        }
    }

    pub(super) fn sample(&self) -> (u64, Duration) {
        self.state.as_ref().map_or((0, Duration::ZERO), |state| {
            (state.count.get(), state.elapsed.get())
        })
    }
}

unsafe extern "C" fn begin(
    _: v8::UnsafeRawIsolatePtr,
    _: v8::GCType,
    _: v8::GCCallbackFlags,
    data: *mut c_void,
) {
    // SAFETY: enable registers a stable Box<State> on this isolate's owner
    // thread; detach removes the hook before that box or isolate can be dropped.
    let state = unsafe { &*data.cast::<State>() };
    state.started.set(Some(Instant::now()));
}

unsafe extern "C" fn end(
    _: v8::UnsafeRawIsolatePtr,
    _: v8::GCType,
    _: v8::GCCallbackFlags,
    data: *mut c_void,
) {
    // SAFETY: same registration/lifetime invariant as begin. V8 documents
    // these callbacks as non-reentrant; an unmatched end adds no sample.
    let state = unsafe { &*data.cast::<State>() };
    if let Some(started) = state.started.take() {
        state
            .elapsed
            .set(state.elapsed.get().saturating_add(started.elapsed()));
        state.count.set(state.count.get().saturating_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirror Agent's teardown even when an assertion unwinds the test. A
    // profile must not free callback data before unregistering from its isolate.
    struct Fixture {
        profile: Profile,
        isolate: v8::OwnedIsolate,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.profile.detach(&mut self.isolate);
        }
    }

    #[test]
    fn gc_callbacks_are_optional_removable_and_survive_moving_the_owner() {
        crate::engine::script::engine::runtime::initialize_v8();
        let mut fixture = Fixture {
            isolate: v8::Isolate::new(v8::CreateParams::default()),
            profile: Profile::default(),
        };
        fixture.isolate.low_memory_notification();
        assert_eq!(fixture.profile.sample(), (0, Duration::ZERO));
        fixture.profile.enable(&mut fixture.isolate, true);
        let mut fixture = Box::new(fixture);
        let Fixture { profile, isolate } = fixture.as_mut();
        isolate.low_memory_notification();
        let sample = profile.sample();
        assert!(sample.0 > 0 && !sample.1.is_zero());
        profile.enable(isolate, true);
        assert_eq!(
            profile.sample(),
            sample,
            "enabling twice must not register twice"
        );
        profile.enable(isolate, false);
        isolate.low_memory_notification();
        assert_eq!(profile.sample(), (0, Duration::ZERO));
        profile.enable(isolate, true);
        isolate.low_memory_notification();
        assert!(profile.sample().0 > 0);
        profile.detach(isolate);
        isolate.low_memory_notification();
    }
}
