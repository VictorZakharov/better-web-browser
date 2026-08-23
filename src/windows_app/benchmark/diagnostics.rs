//! Validation for opt-in selector diagnostics collected by the isolated renderer.

use better_web_browser::limits::{
    MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES, MAX_PAGE_DIAGNOSTIC_SELECTORS,
};

pub(super) fn validate_selector_count(selectors: &[String]) -> Result<(), String> {
    if selectors.len() > MAX_PAGE_DIAGNOSTIC_SELECTORS {
        return Err(format!(
            "at most {MAX_PAGE_DIAGNOSTIC_SELECTORS} --diagnostic-selector options are allowed"
        ));
    }
    if selectors
        .iter()
        .any(|selector| selector.len() > MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES)
    {
        return Err(format!(
            "--diagnostic-selector values cannot exceed {MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES} UTF-8 bytes"
        ));
    }
    Ok(())
}
