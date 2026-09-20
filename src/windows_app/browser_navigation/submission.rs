//! Consume one renderer navigation effect with its method, target, and input provenance.
use super::*;
use better_web_browser::renderer_protocol::RuntimeReport;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn follow_runtime_navigation(
        &mut self,
        report: &RuntimeReport,
        presentation: Option<(better_web_browser::renderer_protocol::DocumentId, u64)>,
    ) -> bool {
        let Some(url) = report.navigation_url.as_ref() else {
            return false;
        };
        let options = &report.navigation_options;
        if let Err(error) = options.validate() {
            self.set_status(error);
            return false;
        }
        let target = options.target.to_ascii_lowercase();
        if !matches!(
            target.as_str(),
            "" | "_self" | "_top" | "_parent" | "_blank"
        ) {
            self.set_status("Navigation to a named browsing context is not supported yet");
            return false;
        }
        let history = if options.user_initiated {
            if options.replace_history {
                HistoryMode::Script
            } else {
                HistoryMode::Push
            }
        } else if options.form_submission {
            if !self.script_navigation.allow_form() {
                self.set_status(
                    "Form navigation blocked: the document navigation limit was reached",
                );
                return false;
            }
            if options.replace_history {
                HistoryMode::Script
            } else {
                HistoryMode::ScriptPush
            }
        } else {
            if !self.allow_script_navigation(url) {
                return false;
            }
            if options.replace_history {
                HistoryMode::Script
            } else {
                HistoryMode::ScriptPush
            }
        };
        let referrer = (!options.noreferrer)
            .then(|| self.current_url().map(str::to_owned))
            .flatten();
        if let Some((document, revision)) = presentation {
            self.acknowledge_renderer_presentation(document, revision, false, false);
        }
        if target == "_blank" {
            let previous = self.tabs.active_id();
            self.new_tab();
            if self.tabs.active_id() == previous {
                // The effect was consumed and its presentation already acknowledged.
                // Keep the tab-limit error without acknowledging the same revision twice.
                return true;
            }
        }
        self.begin_navigation_request_for_tab(
            self.tabs.active_id(),
            url.clone(),
            history,
            referrer,
            options.post.clone(),
        );
        true
    }
}
