//! Native constraint-validation query with V8 scope access.
//!
//! Pattern regexes evaluate on the calling realm's isolate through the scope
//! this dispatch already holds: entering a second isolate while a page
//! isolate runs on this thread is not safe, so validity host ops never touch
//! regexes themselves. The verdict is also stored in control state (with its
//! exact inputs) so selector matching, which has no scope, can reuse it.
//! Callers without any entered isolate use the process pattern tester with
//! [`PatternSource::Live`]; selector matching uses [`PatternSource::Cached`].

use super::bridge::HostBridge;
use crate::engine::dom::node::control_validity::{self, PatternSource};
use crate::engine::invalidation::MutationKind;
use std::cell::RefCell;

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    result.set(v8::undefined(scope).into());
    let context = scope.get_current_context();
    let Some(host) = context
        .get_slot::<HostBridge>()
        .and_then(|bridge| match &*bridge {
            HostBridge::Document(host) => host.upgrade(),
            _ => None,
        })
    else {
        return;
    };
    let Some(id) = arguments.get(1).uint32_value(scope) else {
        return;
    };
    let Some(node) = host.borrow().node(id) else {
        return;
    };
    // The tester borrows the live scope per evaluation. The calling realm's
    // context is already entered, so regexes compile and run in place with
    // no second isolate. Evaluation is pure regex with no DOM access, so it
    // cannot re-enter validation.
    let cell = RefCell::new(scope);
    let tester = |pattern: &str, value: &str| -> Option<bool> {
        let mut guard = cell.borrow_mut();
        // Reborrow through the guard: the scope must stay anchored while used
        // (deref alone would move out of the borrowed guard).
        #[allow(clippy::needless_borrow, clippy::explicit_auto_deref)]
        let scope: &mut v8::PinScope = &mut *guard;
        // A failed compile schedules a SyntaxError on this isolate; contain
        // it so invalid patterns stay a quiet "no constraint".
        v8::tc_scope!(let tc, scope);
        let source = v8::String::new(tc, &format!("^(?:{pattern})$"))?;
        let expression = match v8::RegExp::new(tc, source, v8::RegExpCreationFlags::UNICODE_SETS) {
            Some(expression) => expression,
            None => {
                tc.reset();
                return None;
            }
        };
        let name = v8::String::new(tc, "test")?;
        let test = v8::Local::<v8::Function>::try_from(expression.get(tc, name.into())?).ok()?;
        let argument = v8::String::new(tc, value)?;
        let outcome = test.call(tc, expression.into(), &[argument.into()])?;
        Some(outcome.boolean_value(tc))
    };
    let flags = control_validity::validity_of(&node, &PatternSource::Live(&tester));
    let message = control_validity::validation_message(&node, &flags);
    // Selectors reuse the verdict evaluated here; only inputs carry patterns.
    if node.tag_name() == Some("input") {
        let state = node.input_state_name();
        let value = node.input_value();
        if let Some((pattern, values)) =
            control_validity::current_pattern_inputs(&node, &state, &value)
        {
            let verdict = !flags.pattern_mismatch;
            if control_validity::store_pattern_verdict(&node, &pattern, values, verdict) {
                let document = host.borrow().document.clone();
                host.borrow_mut()
                    .record_mutation(Some(&document), MutationKind::State);
            }
        }
    }
    let scope = cell.into_inner();
    let payload = serde_json::json!({
        "flags": serde_json::from_str::<serde_json::Value>(&flags.to_json())
            .unwrap_or(serde_json::Value::Null),
        "message": message,
    })
    .to_string();
    if let Some(text) = v8::String::new(scope, &payload) {
        result.set(text.into());
    }
}
