//! Small, recoverable profile setting for the browser-wide User-Agent mode.

use better_web_browser::branding::UserAgentMode;
use std::path::Path;

pub(super) fn load(profile: &Path) -> Result<UserAgentMode, String> {
    super::profile_setting::load(profile, "user-agent-mode", UserAgentMode::parse)
}

pub(super) fn save(profile: &Path, mode: UserAgentMode) -> Result<(), String> {
    super::profile_setting::save(profile, "user-agent-mode", mode.setting())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

    fn profile() -> PathBuf {
        std::env::temp_dir().join(format!(
            "breeze-user-agent-test-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn absent_profile_defaults_to_breeze_and_choice_survives_reload() {
        let profile = profile();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Breeze);
        save(&profile, UserAgentMode::Chrome).unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Chrome);
        save(&profile, UserAgentMode::Firefox).unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Firefox);
        fs::remove_dir_all(&profile).unwrap();
    }

    #[test]
    fn damaged_primary_recovers_the_previous_choice() {
        let profile = profile();
        save(&profile, UserAgentMode::Chrome).unwrap();
        save(&profile, UserAgentMode::Firefox).unwrap();
        fs::write(profile.join("user-agent-mode.txt"), "not-a-mode").unwrap();
        assert_eq!(load(&profile).unwrap(), UserAgentMode::Chrome);
        fs::remove_dir_all(&profile).unwrap();
    }
}
