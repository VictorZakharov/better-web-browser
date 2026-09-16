//! Install replacement-parser work without replacing the document's realm or timer queue.
use super::*;

impl DocumentRuntime {
    pub(super) fn prepare_document_streams(&mut self) {
        let prepare = self.written_script_preparation();
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_stream_preparation(prepare);
        }
    }

    pub(super) fn collect_document_stream_changes(&mut self) {
        let Some(update) = self
            .script_runtime
            .as_mut()
            .and_then(ScriptRuntime::take_stream_update)
        else {
            return;
        };
        if update.replaced {
            // Discard old parser input/owners. Already issued network responses may drain,
            // but cannot execute an old parser script in the new document contents.
            self.parser = None;
            self.parser_scripts = Default::default();
            self.page.reset_document_stream();
            self.abandon_document_load_resources();
            self.loaded_resources.clear();
            self.focused_node = None;
            self.rendering = Default::default();
        }
        self.parser_scripts.set_parsing(update.parsing);
        for (script, executed) in update.scripts {
            self.page.scripts.push(script.clone());
            if !executed {
                self.parser_scripts.enqueue(script);
            }
        }
        if update.mutated {
            self.page.dom.quirks_mode.set(update.quirks);
            self.page.discover_parsed_resources();
            self.record_written_stylesheets(update.stylesheets);
            self.sync_script_layout_page();
        }
    }
}
