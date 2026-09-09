//! The embedder fetches module dependencies; this API only compiles/discovers them.
use super::*;
#[cfg(test)]
mod tests;

impl ScriptRuntime {
    pub(crate) fn prepare_module_graph(
        &mut self,
        url: &str,
        source: &str,
    ) -> Result<Vec<String>, String> {
        // Account roots before compiling, not only when a later execution task runs.
        self.install_module_dependency(url, source)?;
        let context = self
            .context
            .as_deref_mut()
            .ok_or("JavaScript realm is inactive")?;
        let sources = self.host.borrow().module_loader.sources();
        context
            .prepare_module(url, source, &sources)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn install_module_dependency(
        &mut self,
        url: &str,
        source: &str,
    ) -> Result<(), String> {
        if self.host.borrow().module_loader.contains(url) {
            return Ok(());
        }
        if source.len() > MAX_SCRIPT_BYTES
            || self.total_script_bytes.saturating_add(source.len()) > MAX_PAGE_SCRIPT_BYTES
        {
            return Err("module graph exceeds the JavaScript byte limit".into());
        }
        if self
            .host
            .borrow()
            .module_loader
            .add_source(url.to_owned(), source.to_owned())
        {
            self.total_script_bytes += source.len();
        }
        Ok(())
    }
}
