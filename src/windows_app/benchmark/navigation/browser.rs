//! A closed set of same-window actions for hidden lifecycle checks.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum BrowserAction {
    NewTab,
    NextTab,
    PreviousTab,
    CloseTab,
    Reader,
}

impl BrowserAction {
    pub(in crate::windows_app) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "new-tab" => Ok(Self::NewTab),
            "next-tab" => Ok(Self::NextTab),
            "previous-tab" => Ok(Self::PreviousTab),
            "close-tab" => Ok(Self::CloseTab),
            "reader" => Ok(Self::Reader),
            _ => Err(
                "hidden browser action must be new-tab, next-tab, previous-tab, close-tab or reader"
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
        match action {
            BrowserAction::NewTab => self.new_tab(),
            BrowserAction::Reader => self.toggle_reader(),
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
        for value in ["new-tab", "next-tab", "previous-tab", "close-tab", "reader"] {
            assert!(BrowserAction::parse(value).is_ok());
        }
        for value in [
            "new-window",
            "close-window",
            "reload",
            "",
            "NEW-TAB",
            "new-tab,close-window",
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
