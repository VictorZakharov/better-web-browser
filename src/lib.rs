pub mod branding;
pub mod broadcast_channel;
pub mod cache_storage;
pub mod document;
pub(crate) mod encoded_audio;
pub mod engine;
pub mod fetch;
pub mod fuzzing;
pub mod indexed_db;
pub mod limits;
pub mod metrics;
pub mod navigation;
pub(crate) mod process_memory;
pub mod protocol_handlers;
pub mod renderer_budget;
pub mod renderer_protocol;
pub mod storage;
pub mod storage_manager;
pub(crate) mod text_decode;

#[cfg(target_os = "windows")]
// The broker is deliberately private until a browser permission service can own its grants.
#[allow(dead_code)]
pub mod capture_process;
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub(crate) mod capture_protocol;
#[cfg(target_os = "windows")]
pub(crate) mod media_data_protocol;
// Shared wire types are also consumed by the platform-independent media protocol.
pub(crate) mod media_frame_protocol;
#[cfg(target_os = "windows")]
pub mod media_process;
pub mod media_protocol;
pub(crate) mod media_type;

pub(crate) mod iso_bmff_audio;
pub(crate) mod ogg_vorbis_headers;
pub(crate) mod opus_audio;
pub(crate) mod webm_opus;

#[cfg(target_os = "windows")]
pub mod renderer_process;

#[cfg(target_os = "windows")]
pub mod winhttp;
