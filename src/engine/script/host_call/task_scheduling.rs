use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "timerSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("timer identifiers must be positive integers")
                    .into());
            }
            let delay = argument_duration(args, 2);
            let repeat = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            state.schedule_timer(id, delay, repeat);
            JsValue::from(id)
        }
        "mediaTaskSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("task identifiers must be positive integers")
                    .into());
            }
            state.schedule_media_task(id);
            JsValue::from(id)
        }
        "idleSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("idle callback identifiers must be positive integers")
                    .into());
            }
            // Timeouts start at registration, not at the beginning of a long-running task.
            let now = state.timers.now().saturating_add(
                state
                    .task_started
                    .map(|started| started.elapsed())
                    .unwrap_or_default(),
            );
            state
                .idle_callbacks
                .schedule(id, now, argument_duration(args, 2));
            JsValue::from(id)
        }
        "idleCancel" => {
            state.idle_callbacks.cancel(argument_id(args, 1));
            JsValue::undefined()
        }
        "idleTimeRemaining" => {
            let next_task = state.timers.next_due_time();
            JsValue::from(state.idle_callbacks.remaining(
                argument_id(args, 1),
                state.timers.now(),
                next_task,
                state.idle_blocked(),
            ))
        }
        "timerCancel" => {
            let cancelled = state.cancel_timer(argument_id(args, 1));
            JsValue::from(cancelled)
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
