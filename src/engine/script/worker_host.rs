//! Native state and host calls for an isolated dedicated-worker realm.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
use serde::Serialize;
use std::sync::Arc;

pub type WorkerSourceLoader =
    dyn Fn(&str, ScriptKind) -> Result<String, String> + Send + Sync + 'static;

pub(super) struct WorkerHostState {
    pub(super) source_url: String,
    pub(super) name: String,
    pub(super) kind: ScriptKind,
    pub(super) source_loader: Arc<WorkerSourceLoader>,
    pub(super) next_fetch_id: u32,
    pub(super) fetch_actions: Vec<ScriptFetchAction>,
    pub(super) messages: Vec<String>,
    pub(super) console: Vec<String>,
    pub(super) closed: bool,
    pub(super) module_evaluation_pending: bool,
    pub(super) module_evaluation_completion: Option<Result<(), String>>,
    pub(super) timers: EventLoopScheduler<u32>,
    pub(super) timer_handles: HashMap<u32, TaskHandle>,
    pub(super) imported_script_bytes: usize,
}

#[derive(Serialize)]
struct ImportedScript {
    url: String,
    code: String,
}

impl WorkerHostState {
    pub(super) fn new(
        source_url: &str,
        name: &str,
        kind: ScriptKind,
        source_loader: Arc<WorkerSourceLoader>,
    ) -> Self {
        Self {
            source_url: source_url.into(),
            name: name.into(),
            kind,
            source_loader,
            next_fetch_id: 1,
            fetch_actions: Vec::new(),
            messages: Vec::new(),
            console: Vec::new(),
            closed: false,
            module_evaluation_pending: false,
            module_evaluation_completion: None,
            timers: EventLoopScheduler::new(),
            timer_handles: HashMap::new(),
            imported_script_bytes: 0,
        }
    }

    pub(super) fn schedule_timer(&mut self, id: u32, delay: Duration, repeat: bool) {
        if let Some(previous) = self.timer_handles.remove(&id) {
            self.timers.cancel(previous);
        }
        let handle = if repeat {
            self.timers.queue_repeating_task(
                TaskSource::Timer,
                delay,
                delay.max(Duration::from_millis(1)),
                id,
            )
        } else {
            self.timers.queue_task(TaskSource::Timer, delay, id)
        };
        self.timer_handles.insert(id, handle);
    }

    pub(super) fn take_ready_timer(&mut self) -> Option<u32> {
        let mut ready = None;
        self.timers.run_one_task(|_, work| {
            if let ScheduledWork::Task(task) = work {
                ready = Some((task.payload, task.repeating));
            }
        });
        let (id, repeating) = ready?;
        if !repeating {
            self.timer_handles.remove(&id);
        }
        Some(id)
    }
}

pub(super) fn dispatch_worker_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut WorkerHostState,
) -> JsResult<JsValue> {
    match operation {
        "workerModuleComplete" => {
            let succeeded = args.get(2).and_then(JsValue::as_boolean).unwrap_or(false);
            let reason = argument_string(args, 3)?;
            state.module_evaluation_completion = Some(if succeeded {
                Ok(())
            } else {
                Err(if reason.is_empty() {
                    "Worker module evaluation rejected".into()
                } else {
                    reason
                })
            });
            Ok(JsValue::undefined())
        }
        "resolveUrl" => {
            let value = argument_string(args, 1)?;
            Ok(js_string(
                resolve_url(&state.source_url, &value).unwrap_or(value),
            ))
        }
        "strictResolveUrl" => {
            let value = argument_string(args, 1)?;
            let base = if args.len() > 2 {
                argument_string(args, 2)?
            } else {
                state.source_url.clone()
            };
            let resolved = crate::navigation::resolve_web_url(&base, &value).ok_or_else(|| {
                JsNativeError::typ().with_message(format!("Invalid URL: {value}"))
            })?;
            Ok(js_string(resolved))
        }
        "parseWebUrl" => {
            let value = argument_string(args, 1)?;
            let parts = crate::navigation::web_url_parts(&value).ok_or_else(|| {
                JsNativeError::typ().with_message(format!("Invalid URL: {value}"))
            })?;
            Ok(js_string(
                serde_json::to_string(&parts).expect("URL parts serialize"),
            ))
        }
        "setWebUrlComponent" => {
            let value = argument_string(args, 1)?;
            let component = argument_string(args, 2)?;
            let input = argument_string(args, 3)?;
            let resolved = crate::navigation::set_web_url_component(&value, &component, &input)
                .ok_or_else(|| {
                    JsNativeError::typ().with_message(format!("Invalid URL {component}: {input}"))
                })?;
            Ok(js_string(resolved))
        }
        "workerLocation" => Ok(js_string(state.source_url.clone())),
        "workerName" => Ok(js_string(state.name.clone())),
        "userAgent" => Ok(js_string(crate::branding::USER_AGENT.to_string())),
        "console" => {
            let level = argument_string(args, 1)?;
            let message = argument_string(args, 2)?;
            state.console.push(format!("{level}: {message}"));
            Ok(JsValue::undefined())
        }
        "fetchStart" => {
            let serialized = argument_string(args, 1)?;
            let request = super::network::request_from_serialized(&state.source_url, &serialized)?;
            let id = state.next_fetch_id;
            state.next_fetch_id = state.next_fetch_id.checked_add(1).ok_or_else(|| {
                JsNativeError::range().with_message("Worker Fetch identifiers were exhausted")
            })?;
            state.fetch_actions.push(ScriptFetchAction::Start {
                id,
                request: Box::new(request),
            });
            Ok(JsValue::from(id))
        }
        "fetchAbort" => {
            state.fetch_actions.push(ScriptFetchAction::Abort {
                id: argument_id(args, 1),
            });
            Ok(JsValue::undefined())
        }
        "workerPost" => {
            state.messages.push(argument_string(args, 1)?);
            Ok(JsValue::undefined())
        }
        "workerClose" => {
            state.closed = true;
            Ok(JsValue::undefined())
        }
        "timerSchedule" => {
            let id = argument_id(args, 1);
            let delay = super::binding_helpers::argument_duration(args, 2);
            let repeat = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            state.schedule_timer(id, delay, repeat);
            Ok(JsValue::from(id))
        }
        "timerCancel" => {
            let cancelled = state
                .timer_handles
                .remove(&argument_id(args, 1))
                .is_some_and(|handle| state.timers.cancel(handle));
            Ok(JsValue::from(cancelled))
        }
        "workerImportScripts" => import_scripts(args, state),
        _ => Err(JsNativeError::typ()
            .with_message(format!("unsupported Worker host operation: {operation}"))
            .into()),
    }
}

fn import_scripts(args: &[JsValue], state: &mut WorkerHostState) -> JsResult<JsValue> {
    if state.kind == ScriptKind::Module {
        return Err(JsNativeError::typ()
            .with_message("importScripts is unavailable in module workers")
            .into());
    }
    let urls: Vec<String> = serde_json::from_str(&argument_string(args, 1)?)
        .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
    let mut scripts = Vec::new();
    for value in urls {
        let url = resolve_url(&state.source_url, &value).ok_or_else(|| {
            JsNativeError::typ().with_message(format!("invalid importScripts URL: {value}"))
        })?;
        let code = (state.source_loader)(&url, ScriptKind::Classic)
            .map_err(|error| JsNativeError::error().with_message(error))?;
        if state.imported_script_bytes.saturating_add(code.len()) > MAX_SCRIPT_BYTES {
            return Err(JsNativeError::range()
                .with_message("importScripts exceeded the Worker JavaScript byte limit")
                .into());
        }
        state.imported_script_bytes += code.len();
        scripts.push(ImportedScript { url, code });
    }
    serde_json::to_string(&scripts)
        .map(js_string)
        .map_err(|error| {
            JsNativeError::error()
                .with_message(error.to_string())
                .into()
        })
}
