//! Bounded navigation-chain state for renderer-requested document replacements.

use super::BrowserState;
use better_web_browser::limits::MAX_SCRIPT_NAVIGATIONS;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub(super) struct ScriptNavigationGuard {
    followed: usize,
    visited: HashSet<String>,
}

impl ScriptNavigationGuard {
    pub(super) fn reset(&mut self, initial_url: &str) {
        self.followed = 0;
        self.visited.clear();
        self.visited.insert(initial_url.to_string());
    }

    pub(super) fn record_committed(&mut self, final_url: &str) {
        self.visited.insert(final_url.to_string());
    }

    pub(super) fn allow(&mut self, target: &str) -> Result<(), &'static str> {
        if self.followed >= MAX_SCRIPT_NAVIGATIONS {
            return Err("the document navigation limit was reached");
        }
        if !self.visited.insert(target.to_string()) {
            return Err("the target already appeared in this navigation chain");
        }
        self.followed += 1;
        Ok(())
    }
}

impl BrowserState {
    pub(super) unsafe fn allow_script_navigation(&mut self, target: &str) -> bool {
        match self.script_navigation.allow(target) {
            Ok(()) => true,
            Err(error) => {
                self.set_status(&format!("Script navigation blocked: {error}"));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_revisited_script_navigation_targets() {
        let mut guard = ScriptNavigationGuard::default();
        guard.reset("https://example.com/start");
        assert!(guard.allow("https://example.com/next").is_ok());
        assert!(guard.allow("https://example.com/start").is_err());
        assert!(guard.allow("https://example.com/next").is_err());
    }

    #[test]
    fn caps_each_script_navigation_chain_and_resets_for_user_navigation() {
        let mut guard = ScriptNavigationGuard::default();
        guard.reset("https://example.com/start");
        for index in 0..MAX_SCRIPT_NAVIGATIONS {
            assert!(guard.allow(&format!("https://example.com/{index}")).is_ok());
        }
        assert!(guard.allow("https://example.com/overflow").is_err());

        guard.reset("https://example.com/user-navigation");
        assert!(guard.allow("https://example.com/fresh").is_ok());
    }
}
