//! Browser-owned child launch arguments, separate from protocol execution.
use super::{Nonce, RendererSessionId, StartupFault, media};

pub(super) struct ChildOptions {
    pub(super) nonce: Nonce,
    pub(super) session: RendererSessionId,
    pub(super) user_agent_mode: crate::branding::UserAgentMode,
    pub(super) memory_budget: crate::renderer_budget::RendererBudget,
    pub(super) test_mode: bool,
    pub(super) fault: Option<StartupFault>,
    pub(super) media: Option<media::ChildMediaOptions>,
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
        let nonce = Nonce::from_hex(value("--renderer-nonce")?)
            .map_err(|error| format!("parse renderer nonce: {error}"))?;
        let session = value("--renderer-session")?
            .parse::<u64>()
            .map_err(|_| "--renderer-session requires an integer".to_string())
            .and_then(|value| RendererSessionId::new(value).map_err(|error| error.to_string()))?;
        let user_agent_mode = arguments
            .iter()
            .any(|argument| argument == "--renderer-user-agent")
            .then(|| value("--renderer-user-agent"))
            .transpose()?
            .map(|value| {
                crate::branding::UserAgentMode::parse(value)
                    .ok_or_else(|| format!("unknown renderer User-Agent mode: {value}"))
            })
            .transpose()?
            .unwrap_or_default();
        let fault = arguments
            .iter()
            .any(|argument| argument == "--renderer-startup-fault")
            .then(|| value("--renderer-startup-fault"))
            .transpose()?
            .map(|fault| match fault.as_str() {
                "silent" => Ok(StartupFault::Silent),
                "wrong-nonce" => Ok(StartupFault::WrongNonce),
                "malformed" => Ok(StartupFault::MalformedFrame),
                "oversized" => Ok(StartupFault::OversizedFrame),
                "incompatible" => Ok(StartupFault::IncompatibleVersion),
                _ => Err(format!("unknown renderer startup fault: {fault}")),
            })
            .transpose()?;
        let memory_budget = arguments
            .iter()
            .any(|argument| argument == "--renderer-memory-budget")
            .then(|| value("--renderer-memory-budget"))
            .transpose()?
            .map(|value| {
                crate::renderer_budget::RendererBudget::parse(value)
                    .ok_or_else(|| "unknown renderer memory budget".to_string())
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            nonce,
            session,
            user_agent_mode,
            memory_budget,
            test_mode: arguments
                .iter()
                .any(|argument| argument == "--renderer-test-mode"),
            fault,
            media: media::ChildMediaOptions::parse(arguments)?,
        })
    }
}
