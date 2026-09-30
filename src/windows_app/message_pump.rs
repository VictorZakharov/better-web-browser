//! Bounded native input priority without discarding or compacting window messages.
use super::*;

const MAX_PREFERRED_INPUTS: u8 = 8;
const PM_REMOVE: u32 = 0x0001;
// Modern QS_INPUT includes keyboard, mouse, raw, touch and pointer messages.
// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getqueuestatus
const QS_INPUT: u32 = 0x1c07;
const PM_QS_INPUT: u32 = QS_INPUT << 16;
// Only posted characters are inspected here, not the SDK's broader
// PM_QS_POSTMESSAGE mask, which also includes timers and hotkeys.
const PM_QS_POSTED_CHARACTERS: u32 = 0x0008 << 16;
const POSTED_CHARACTER_MESSAGES: [u32; 5] = [0x0102, 0x0103, 0x0106, 0x0107, 0x0109];
const WM_QUIT: u32 = 0x0012;

pub(super) fn input_is_waiting() -> bool {
    // The high word describes queued input, not just newly-arrived input. This
    // is a non-consuming scheduling hint; PeekMessage still owns actual retrieval.
    unsafe { (GetQueueStatus(QS_INPUT) >> 16) & QS_INPUT != 0 }
}

#[derive(Default)]
struct InputFirstPolicy {
    preferred_reads: u8,
    translated_input_pending: bool,
}

impl InputFirstPolicy {
    fn note_translation(&mut self, translated: bool) {
        self.translated_input_pending |= translated;
    }

    fn next<T, E>(
        &mut self,
        mut input: impl FnMut() -> Option<T>,
        ordinary: impl FnOnce() -> Result<Option<T>, E>,
        posted_character_is_waiting: impl FnOnce() -> bool,
    ) -> Result<Option<T>, E> {
        if self.translated_input_pending {
            self.translated_input_pending = posted_character_is_waiting();
        }
        if !self.translated_input_pending
            && self.preferred_reads < MAX_PREFERRED_INPUTS
            && let Some(message) = input()
        {
            self.preferred_reads += 1;
            return Ok(Some(message));
        }
        // Give ordinary posted work a turn after a finite burst. When input is
        // empty, GetMessage blocks rather than repeatedly peeking an empty queue.
        self.preferred_reads = 0;
        ordinary()
    }
}

pub(super) unsafe fn run(app: &BrowserApplication) -> Result<(), String> {
    let mut policy = InputFirstPolicy::default();
    while let Some(message) = policy.next(peek_input, read_ordinary, posted_character_is_waiting)? {
        // PeekMessage can retrieve WM_QUIT too; it must never be dispatched.
        if message.message == WM_QUIT {
            break;
        }
        let handled = app
            .browser_for_message(message.hwnd)
            .is_some_and(|(window, state)| dispatch_browser_input(&message, window, &mut *state));
        if !handled {
            // Translation posts characters behind preceding posted work. Keep
            // ordinary retrieval until every UTF-16/dead/system character drains;
            // later keys or focus-changing input must not overtake that edit.
            // TranslateMessage also returns nonzero for untranslated key messages.
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-translatemessage
            policy.note_translation(TranslateMessage(&message) != 0);
            DispatchMessageW(&message);
        }
        // Window procedures have returned; only the surviving foreground state
        // for this message may service renderer work, a scroll frame or video.
        renderer_lifecycle::flush_due_for_message(app, message.hwnd);
        scrolling::frame_service::flush_for_message(app, message.hwnd);
        runtime::flush_due_for_message(app, message.hwnd);
        video_presentation::flush_for_message(app, message.hwnd);
    }
    Ok(())
}

fn posted_character_is_waiting() -> bool {
    POSTED_CHARACTER_MESSAGES.into_iter().any(|kind| unsafe {
        let mut message = std::mem::zeroed();
        // Inspect posted queue sources without removing a character or preceding
        // posted work. Exact ranges exclude intervening system-key messages.
        // The five kinds are CHAR, DEADCHAR, SYSCHAR, SYSDEADCHAR and UNICHAR.
        PeekMessageW(
            &mut message,
            null_mut(),
            kind,
            kind,
            PM_QS_POSTED_CHARACTERS,
        ) != 0
            && message.message == kind
    })
}

fn peek_input() -> Option<Msg> {
    unsafe {
        let mut message = std::mem::zeroed();
        // Queue-source flags select actual input, not posted lookalikes with a
        // mouse message number. Both APIs still service sent messages normally.
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-peekmessagew
        (PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE | PM_QS_INPUT) != 0)
            .then_some(message)
    }
}

fn read_ordinary() -> Result<Option<Msg>, String> {
    unsafe {
        let mut message = std::mem::zeroed();
        match GetMessageW(&mut message, null_mut(), 0, 0) {
            0 => Ok(None),
            value if value < 0 => Err(last_error("read window message")),
            _ => Ok(Some(message)),
        }
    }
}

#[cfg(test)]
mod tests;
