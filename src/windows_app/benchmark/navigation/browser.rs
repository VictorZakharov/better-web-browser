//! A closed set of same-window actions for hidden lifecycle checks.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum BrowserAction {
    NewTab,
    NextTab,
    PreviousTab,
    CloseTab,
    Reader,
    Fullscreen,
    Resize { width: i32, height: i32 },
}

impl BrowserAction {
    pub(in crate::windows_app) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "new-tab" => Ok(Self::NewTab),
            "next-tab" => Ok(Self::NextTab),
            "previous-tab" => Ok(Self::PreviousTab),
            "close-tab" => Ok(Self::CloseTab),
            "reader" => Ok(Self::Reader),
            "fullscreen" => Ok(Self::Fullscreen),
            value if value.starts_with("resize=") => {
                let (width, height) = value[7..].split_once('x').ok_or("hidden resize requires WIDTHxHEIGHT")?;
                let dimension = |value: &str, minimum, maximum| {
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err("hidden resize dimensions must be decimal integers".to_string());
                    }
                    value.parse::<i32>().ok().filter(|value| (minimum..=maximum).contains(value))
                        .ok_or_else(|| "hidden resize dimensions exceed the supported window bounds".to_string())
                };
                Ok(Self::Resize { width: dimension(width, 320, 7680)?, height: dimension(height, 240, 4320)? })
            }
            _ => Err(
                "hidden browser action must be a tab action, reader, fullscreen or resize=WIDTHxHEIGHT"
                    .into(),
            ),
        }
    }
}

impl BrowserState {
    pub(super) unsafe fn run_hidden_browser_action(
        &mut self,
        action: BrowserAction,
    ) -> Result<(), String> {
        if self.benchmark.is_none() {
            return Err("browser actions require hidden benchmark mode".into());
        }
        if IsWindowVisible(self.window) != 0 {
            return Err("hidden browser actions require an invisible native window".into());
        }
        match action {
            BrowserAction::NewTab => self.new_tab(),
            BrowserAction::Reader => self.toggle_reader(),
            BrowserAction::Fullscreen => self.toggle_browser_fullscreen(),
            BrowserAction::Resize { width, height } => {
                let mut rect = Rect::default();
                if GetWindowRect(self.window, &mut rect) == 0
                    || SetWindowPos(
                        self.window,
                        std::ptr::null_mut(),
                        rect.left,
                        rect.top,
                        scale_dip(width, self.dpi),
                        scale_dip(height, self.dpi),
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    ) == 0
                {
                    return Err("hidden native window resize failed".into());
                }
            }
            BrowserAction::NextTab | BrowserAction::PreviousTab | BrowserAction::CloseTab => {
                if self.tabs.len() < 2 {
                    return Err("hidden tab action requires at least two tabs".into());
                }
                match action {
                    BrowserAction::NextTab => self.activate_relative_tab(true),
                    BrowserAction::PreviousTab => self.activate_relative_tab(false),
                    BrowserAction::CloseTab => self.close_tab(self.tabs.active_id()),
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_browser_actions_cannot_open_or_close_a_native_window() {
        for value in [
            "new-tab",
            "next-tab",
            "previous-tab",
            "close-tab",
            "reader",
            "fullscreen",
            "resize=1550x1036",
        ] {
            assert!(BrowserAction::parse(value).is_ok());
        }
        for value in [
            "new-window",
            "close-window",
            "reload",
            "",
            "NEW-TAB",
            "new-tab,close-window",
            "resize=319x240",
            "resize=320x239",
            "resize=7681x4320",
            "resize=7680x4321",
            "resize=+320x240",
            "resize=320x240x240",
            "resize=2147483648x240",
        ] {
            assert!(BrowserAction::parse(value).is_err());
        }
        assert!(
            super::super::super::options::LaunchOptions::parse_from(
                Instant::now(),
                ["--browser-action-after-ready", "new-tab"].map(str::to_owned)
            )
            .is_err()
        );
    }
}
