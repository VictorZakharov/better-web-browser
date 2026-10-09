//! The selected budget applies on restart; never mutate a running Job/realm.
use better_web_browser::renderer_budget::RendererBudget;
use std::path::Path;

pub(super) fn load(profile: &Path) -> Result<RendererBudget, String> {
    super::profile_setting::load(profile, "renderer-memory-budget", RendererBudget::parse)
}

pub(super) fn save(profile: &Path, budget: RendererBudget) -> Result<(), String> {
    super::profile_setting::save(profile, "renderer-memory-budget", budget.setting())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn memory_choice_survives_restart_and_recovers_the_previous_choice() {
        let profile =
            std::env::temp_dir().join(format!("breeze-memory-setting-{}", std::process::id()));
        assert_eq!(load(&profile).unwrap(), RendererBudget::Standard);
        save(&profile, RendererBudget::Graphics).unwrap();
        assert_eq!(load(&profile).unwrap(), RendererBudget::Graphics);
        save(&profile, RendererBudget::Standard).unwrap();
        fs::write(profile.join("renderer-memory-budget.txt"), "unlimited").unwrap();
        assert_eq!(load(&profile).unwrap(), RendererBudget::Graphics);
        // Stale temporary writes are not settings; removing both committed files
        // returns to the conservative default rather than an old opt-in.
        fs::remove_file(profile.join("renderer-memory-budget.txt")).unwrap();
        fs::remove_file(profile.join("renderer-memory-budget.bak")).unwrap();
        fs::write(profile.join("renderer-memory-budget.tmp-stale"), "graphics").unwrap();
        assert_eq!(load(&profile).unwrap(), RendererBudget::Standard);
        fs::write(profile.join("renderer-memory-budget.txt"), vec![b'x'; 33]).unwrap();
        assert!(load(&profile).is_err());
        fs::write(profile.join("renderer-memory-budget.txt"), [0xff]).unwrap();
        assert!(load(&profile).is_err());
        fs::remove_dir_all(profile).unwrap();
    }
}
