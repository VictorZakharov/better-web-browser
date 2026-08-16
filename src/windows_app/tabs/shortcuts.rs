#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::windows_app) struct KeyModifiers {
    pub(in crate::windows_app) control: bool,
    pub(in crate::windows_app) shift: bool,
    pub(in crate::windows_app) alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum BrowserShortcut {
    NewTab,
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    ActivatePosition(usize),
    ActivateLast,
    FocusAddress,
    Reload,
    Back,
    Forward,
}

pub(in crate::windows_app) fn shortcut_for_key(
    key: usize,
    modifiers: KeyModifiers,
) -> Option<BrowserShortcut> {
    const VK_TAB: usize = 0x09;
    const VK_F5: usize = 0x74;
    const VK_PRIOR: usize = 0x21;
    const VK_NEXT: usize = 0x22;
    const VK_LEFT: usize = 0x25;
    const VK_RIGHT: usize = 0x27;
    const VK_1: usize = b'1' as usize;
    const VK_8: usize = b'8' as usize;
    const VK_9: usize = b'9' as usize;
    const VK_L: usize = b'L' as usize;
    const VK_R: usize = b'R' as usize;
    const VK_T: usize = b'T' as usize;
    const VK_W: usize = b'W' as usize;

    if key == VK_F5 && !modifiers.alt {
        return Some(BrowserShortcut::Reload);
    }
    if modifiers.alt && !modifiers.control {
        return match key {
            VK_LEFT => Some(BrowserShortcut::Back),
            VK_RIGHT => Some(BrowserShortcut::Forward),
            _ => None,
        };
    }
    if modifiers.control && !modifiers.alt {
        return match key {
            VK_T if modifiers.shift => Some(BrowserShortcut::ReopenClosedTab),
            VK_T => Some(BrowserShortcut::NewTab),
            VK_W if !modifiers.shift => Some(BrowserShortcut::CloseTab),
            VK_L => Some(BrowserShortcut::FocusAddress),
            VK_R => Some(BrowserShortcut::Reload),
            VK_TAB if modifiers.shift => Some(BrowserShortcut::PreviousTab),
            VK_TAB | VK_NEXT => Some(BrowserShortcut::NextTab),
            VK_PRIOR => Some(BrowserShortcut::PreviousTab),
            VK_1..=VK_8 => Some(BrowserShortcut::ActivatePosition(key - b'0' as usize)),
            VK_9 => Some(BrowserShortcut::ActivateLast),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_contract_matches_desktop_browsers() {
        let control = KeyModifiers {
            control: true,
            ..KeyModifiers::default()
        };
        let control_shift = KeyModifiers {
            control: true,
            shift: true,
            alt: false,
        };
        assert_eq!(
            shortcut_for_key(b'T' as usize, control),
            Some(BrowserShortcut::NewTab)
        );
        assert_eq!(
            shortcut_for_key(b'T' as usize, control_shift),
            Some(BrowserShortcut::ReopenClosedTab)
        );
        assert_eq!(
            shortcut_for_key(b'7' as usize, control),
            Some(BrowserShortcut::ActivatePosition(7))
        );
        assert_eq!(
            shortcut_for_key(0x09, control_shift),
            Some(BrowserShortcut::PreviousTab)
        );
        assert_eq!(
            shortcut_for_key(b'W' as usize, control),
            Some(BrowserShortcut::CloseTab)
        );
        assert_eq!(
            shortcut_for_key(b'9' as usize, control),
            Some(BrowserShortcut::ActivateLast)
        );
        assert_eq!(
            shortcut_for_key(0x22, control),
            Some(BrowserShortcut::NextTab)
        );
        assert_eq!(
            shortcut_for_key(0x21, control),
            Some(BrowserShortcut::PreviousTab)
        );
        assert_eq!(
            shortcut_for_key(
                0x25,
                KeyModifiers {
                    alt: true,
                    ..KeyModifiers::default()
                }
            ),
            Some(BrowserShortcut::Back)
        );
        assert_eq!(
            shortcut_for_key(0x74, KeyModifiers::default()),
            Some(BrowserShortcut::Reload)
        );
        assert_eq!(shortcut_for_key(b'W' as usize, control_shift), None);
    }
}
