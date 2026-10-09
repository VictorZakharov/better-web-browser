//! V8 ownership and the engine-neutral values used by native Web API bindings.

mod agent;
mod binary_clone;
mod bridge;
mod control_validation;
mod cpu_samples;
pub(in crate::engine::script) mod crypto;
mod dynamic_imports;
mod event_handlers;
pub(super) mod frames;
#[cfg(windows)]
mod gamepad;
mod gc_profile;
mod memory_pressure;
mod message_clone;
mod messaging;
mod modules;
mod node_wrappers;
pub(in crate::engine::script) use crate::engine::owner_cpu;
mod parser_scripts;
mod policy;
mod ports;
pub(crate) mod runtime;
mod stack_boundary;
mod task_profile;
mod v8_api;
mod value;
pub(crate) mod watchdog;
#[cfg(target_os = "windows")]
mod web_crypto;
mod window_access;
mod worker_packets;

pub(super) use bridge::HostBridge;
pub(super) use runtime::{Context, ModuleEvaluation};
pub(super) use value::{JsError, JsNativeError, JsResult, JsString, JsValue, Source};
