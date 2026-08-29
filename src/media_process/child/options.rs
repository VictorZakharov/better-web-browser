use super::super::launcher::MediaStartupFault;
use crate::media_protocol::{MediaSessionId, Nonce};

pub(super) struct ChildOptions {
    pub(super) nonce: Nonce,
    pub(super) session: MediaSessionId,
    pub(super) data_handle: usize,
    pub(super) frame_handle: usize,
    pub(super) test_mode: bool,
    pub(super) fault: Option<MediaStartupFault>,
}

impl ChildOptions {
    pub(super) fn parse(arguments: &[String]) -> Result<Self, String> {
        let value = |name: &str| {
            arguments
                .iter()
                .position(|argument| argument == name)
                .and_then(|index| arguments.get(index + 1))
                .ok_or_else(|| format!("{name} requires a value"))
        };
        let nonce = Nonce::from_hex(value("--media-nonce")?)
            .map_err(|error| format!("parse media nonce: {error}"))?;
        let session = value("--media-session")?
            .parse::<u64>()
            .map_err(|_| "--media-session requires an integer".to_string())
            .and_then(|value| MediaSessionId::new(value).map_err(|error| error.to_string()))?;
        let data_handle = nonzero_handle(value("--media-data-handle")?, "--media-data-handle")?;
        let frame_handle = nonzero_handle(value("--media-frame-handle")?, "--media-frame-handle")?;
        let fault = arguments
            .iter()
            .any(|argument| argument == "--media-startup-fault")
            .then(|| value("--media-startup-fault"))
            .transpose()?
            .map(|fault| match fault.as_str() {
                "silent" => Ok(MediaStartupFault::Silent),
                "wrong-nonce" => Ok(MediaStartupFault::WrongNonce),
                "malformed" => Ok(MediaStartupFault::MalformedFrame),
                "oversized" => Ok(MediaStartupFault::OversizedFrame),
                "incompatible" => Ok(MediaStartupFault::IncompatibleVersion),
                _ => Err(format!("unknown media startup fault: {fault}")),
            })
            .transpose()?;
        Ok(Self {
            nonce,
            session,
            data_handle,
            frame_handle,
            test_mode: arguments
                .iter()
                .any(|argument| argument == "--media-test-mode"),
            fault,
        })
    }
}

fn nonzero_handle(value: &str, name: &str) -> Result<usize, String> {
    let handle = value
        .parse::<usize>()
        .map_err(|_| format!("{name} requires an integer"))?;
    (handle != 0)
        .then_some(handle)
        .ok_or_else(|| format!("{name} requires a nonzero handle"))
}
