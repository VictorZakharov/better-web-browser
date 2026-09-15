//! Network input and the one encoding restart allowed by tentative-to-certain decoding.
use super::*;
use crate::engine::dom::incremental::HtmlParser;
use crate::winhttp::{DecodedText, DocumentDecoder};

pub(super) struct StreamingInput {
    pub(super) decoder: DocumentDecoder,
    pub(super) source: String,
    pub(super) script_state: Option<crate::engine::script::runtime::RestartState>,
    start: DocumentStart,
    state: DocumentState,
    restart: Option<DecodedText>,
}

pub(super) fn make_parser(
    source: &str,
    input: Option<&StreamingInput>,
) -> Result<HtmlParser, String> {
    if let Some(input) = input {
        let mut parser = HtmlParser::streaming();
        parser.append(source, input.decoder.ended())?;
        Ok(parser)
    } else {
        Ok(HtmlParser::new(source))
    }
}

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn load_stream(
        start: DocumentStart,
        state: DocumentState,
        decoder: DocumentDecoder,
        decoded: DecodedText,
        connection: &mut ChildConnection,
        text: RendererTextSystem,
    ) -> Result<LoadResult, String> {
        let input = StreamingInput {
            decoder,
            source: decoded.text.clone(),
            script_state: None,
            start: start.clone(),
            state: state.clone(),
            restart: None,
        };
        Self::load_source(
            start,
            state,
            decoded,
            Some(input),
            connection,
            text,
            Instant::now(),
        )
    }

    pub(in crate::renderer_process::child) fn append_navigation(
        &mut self,
        bytes: &[u8],
        eof: bool,
    ) -> Result<(), String> {
        let input = self
            .navigation
            .as_mut()
            .ok_or("document has no streaming input")?;
        let decoded = input
            .decoder
            .push(bytes, eof)?
            .ok_or("active document decoder became pending")?;
        input.source.push_str(&decoded.text);
        if let Some(parser) = &mut self.parser {
            parser.parser.append(&decoded.text, eof)?;
        }
        if eof {
            self.reader = crate::document::parse_html(&input.source, &self.page.source_url);
            if self.parser.is_none() {
                self.navigation = None;
            }
        }
        Ok(())
    }

    pub(super) fn parser_encoding(&mut self, label: &str) {
        if let Some(input) = self.navigation.as_mut() {
            input.restart = input.decoder.change_encoding(label);
        }
    }

    pub(super) fn encoding_restart_pending(&self) -> bool {
        self.navigation
            .as_ref()
            .is_some_and(|input| input.restart.is_some())
    }

    pub(in crate::renderer_process::child) fn restart_encoding(
        mut self,
        connection: &mut ChildConnection,
    ) -> Result<LoadResult, String> {
        let mut input = self
            .navigation
            .take()
            .ok_or("missing encoding restart input")?;
        let decoded = input
            .restart
            .take()
            .ok_or("missing encoding restart text")?;
        input.source.clone_from(&decoded.text);
        let revision = self.revision;
        connection.retire_document_fetches(self.id)?;
        let mut retired = ScriptOutcome::default();
        input.script_state = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.take_restart_state(&mut retired));
        connection.send_state_mutations(self.id, &mut retired)?;
        let text = self.into_text();
        let result = Self::load_source(
            input.start.clone(),
            input.state.clone(),
            decoded,
            Some(input),
            connection,
            text,
            Instant::now(),
        )?;
        match result {
            LoadResult::Ready(mut runtime, mut update) => {
                runtime.revision += revision;
                if let AdvanceResult::Presentation(presentation) = &mut update {
                    presentation.revision += revision;
                }
                Ok(LoadResult::Ready(runtime, update))
            }
            other => Ok(other),
        }
    }
}
