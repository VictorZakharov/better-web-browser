//! Browser-owned User-Agent choice; the saved mode applies on the next launch.

use super::*;
use better_web_browser::branding::UserAgentMode;

pub(super) const ID_OPTIONS: usize = 1008;
const ID_UA_BREEZE: usize = 1501;
const ID_UA_CHROME: usize = 1502;
const ID_UA_FIREFOX: usize = 1503;
const MF_STRING: u32 = 0;
const MF_CHECKED: u32 = 0x0008;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const TPM_RETURNCMD: u32 = 0x0100;

const CHOICES: [(usize, UserAgentMode); 3] = [
    (ID_UA_BREEZE, UserAgentMode::Breeze),
    (ID_UA_CHROME, UserAgentMode::Chrome),
    (ID_UA_FIREFOX, UserAgentMode::Firefox),
];

fn mode_for_command(command: usize) -> Option<UserAgentMode> {
    CHOICES
        .iter()
        .find_map(|(id, mode)| (*id == command).then_some(*mode))
}

impl BrowserState {
    pub(super) unsafe fn open_user_agent_options(&mut self) {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            self.set_status(&last_error("create User-Agent options menu"));
            return;
        }
        let selected = self.app.selected_user_agent_mode.get();
        for (id, mode) in CHOICES {
            let label = wide(mode.label());
            let flags = MF_STRING | if mode == selected { MF_CHECKED } else { 0 };
            if AppendMenuW(menu, flags, id, label.as_ptr()) == 0 {
                self.set_status(&last_error("add User-Agent option"));
                DestroyMenu(menu);
                return;
            }
        }

        let mut bounds: Rect = std::mem::zeroed();
        if GetWindowRect(self.controls.options, &mut bounds) == 0 {
            self.set_status(&last_error("locate Options button"));
            DestroyMenu(menu);
            return;
        }
        SetForegroundWindow(self.window);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            bounds.left,
            bounds.bottom,
            0,
            self.window,
            null(),
        ) as usize;
        DestroyMenu(menu);
        let Some(mode) = mode_for_command(command) else {
            return;
        };
        if mode == selected {
            return;
        }
        if let Err(error) = user_agent_preferences::save(&self.app.profile, mode) {
            self.set_status(&format!("Could not save User-Agent option: {error}"));
            return;
        }
        self.app.selected_user_agent_mode.set(mode);
        let message = wide(&format!(
            "Saved {} as the User-Agent mode. Restart Breeze to apply it to requests, pages, and workers.",
            mode.label()
        ));
        let title = wide("Breeze Options");
        MessageBoxW(self.window, message.as_ptr(), title.as_ptr(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_commands_select_only_known_modes() {
        assert_eq!(mode_for_command(ID_UA_BREEZE), Some(UserAgentMode::Breeze));
        assert_eq!(mode_for_command(ID_UA_CHROME), Some(UserAgentMode::Chrome));
        assert_eq!(
            mode_for_command(ID_UA_FIREFOX),
            Some(UserAgentMode::Firefox)
        );
        assert_eq!(mode_for_command(ID_GO), None);
    }
}
