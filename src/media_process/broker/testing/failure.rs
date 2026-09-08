use super::*;
use crate::media_process::broker::MEDIA_EXIT_TIMEOUT;
use crate::renderer_process::windows::{exit_code, wait_for_process};
use std::sync::mpsc;

impl MediaSession {
    #[doc(hidden)]
    pub fn inject_failure(&mut self, command: MediaTestCommand) -> Result<(), String> {
        self.require_test_mode()?;
        if matches!(
            command,
            MediaTestCommand::ProbeRestrictions { .. }
                | MediaTestCommand::WriteMalformedDecodedFrame
                | MediaTestCommand::WriteTruncatedDecodedFrame
                | MediaTestCommand::WriteOversizedDecodedFrame
        ) {
            return Err("use the specialized media test method for this command".into());
        }
        self.send(BrowserMediaMessage::Test(command), "failure injection")?;
        if command == MediaTestCommand::Crash {
            return self.observe_injected_crash();
        }
        match self.incoming.recv_timeout(self.command_timeout) {
            Ok(Ok(message)) => self.protocol_failure(&format!(
                "failure injection unexpectedly returned {message:?}"
            )),
            Ok(Err(error)) => {
                self.mark_exited(
                    format!("media IPC failed after injected fault: {error}"),
                    MEDIA_EXIT_PROTOCOL,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.mark_exited(
                    "media worker exited after injected fault".into(),
                    MEDIA_EXIT_PROTOCOL,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Timeout) if command == MediaTestCommand::Hang => {
                self.mark_exited(
                    "media worker exceeded its command timeout".into(),
                    MEDIA_EXIT_TIMEOUT,
                );
                Ok(())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.mark_exited(
                    "media worker did not surface its injected failure".into(),
                    MEDIA_EXIT_TIMEOUT,
                );
                Err(self.exit_reason.clone().unwrap_or_default())
            }
        }
    }

    fn observe_injected_crash(&mut self) -> Result<(), String> {
        // A crash is a process-lifecycle event, not a promise that the IPC reader thread
        // will be scheduled within the command deadline. Preserve the natural exit code
        // instead of terminating the job again when an asynchronous pipe error arrives.
        if !wait_for_process(&self.process, self.command_timeout) {
            let code = exit_code(&self.process);
            self.mark_exited(
                format!("injected media crash did not terminate the process; exit code: {code:?}"),
                MEDIA_EXIT_TIMEOUT,
            );
            return Err(self.exit_reason.clone().unwrap_or_default());
        }
        self.finish_exit("media worker crashed after injected fault".into());
        match self.exit_code {
            Some(code) if code != 0 => Ok(()),
            code => Err(format!(
                "injected media crash had no abnormal exit code: {code:?}"
            )),
        }
    }
}
