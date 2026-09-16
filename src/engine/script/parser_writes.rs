//! Synchronous parser operations. Host borrows end before author JS or CE reactions run.
use super::binding_helpers::{argument_id, argument_string};
use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};
use crate::engine::page::PageScript;

pub(crate) type PrepareWrittenScript =
    Box<dyn FnMut(NodeRef, usize, &[NodeRef]) -> (Option<PageScript>, bool)>;

pub(super) struct ParserWriteSession {
    pub(super) parser: HtmlParser,
    pub(super) prepare: PrepareWrittenScript,
    pub(super) prepared: Vec<(PageScript, bool)>,
    pub(super) stylesheets: Vec<NodeRef>,
    pub(super) mutated: bool,
    pub(super) initial_count: usize,
    pub(super) root: NodeId,
    pub(super) insertion_point: bool,
    pub(super) paused: bool,
    pub(super) script_bytes: usize,
    pub(super) remaining_script_bytes: usize,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    host: &mut HostState,
) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "parserWriteBegin" => {
            if host
                .parser_write_session
                .as_ref()
                .is_none_or(|session| !session.insertion_point)
                || host
                    .node(argument_id(args, 1))
                    .is_none_or(|node| node.id() != host.document.id())
            {
                return Ok(Some(JsValue::from(false)));
            }
            let text = argument_string(args, 2)?;
            host.ensure_node_capacity(super::mutation_host::estimated_markup_nodes(&text))?;
            let session = host.parser_write_session.as_mut().unwrap();
            session.parser.begin_write(text).map_err(limit_error)?;
            JsValue::from(true)
        }
        "parserWriteStep" => step(host)?,
        "parserWritePrepare" => prepare(host, argument_id(args, 1))?,
        "parserWriteEnd" => {
            if let Some(session) = host.parser_write_session.as_mut() {
                session.parser.end_write();
            }
            JsValue::undefined()
        }
        "parserWriteExecuted" => {
            host.executed += 1;
            JsValue::undefined()
        }
        _ => return Ok(None),
    }))
}

fn step(host: &mut HostState) -> JsResult<JsValue> {
    let Some(session) = host.parser_write_session.as_mut() else {
        return Ok(JsValue::Null);
    };
    if session.paused || host.navigation_url.is_some() {
        return Ok(JsValue::Null);
    }
    let version = session.parser.dom().mutation_version();
    let previous: Vec<_> = Node::descendants(&host.document)
        .filter(crate::engine::page::is_stylesheet)
        .map(|node| (node.id(), node.subtree_mutation_version()))
        .collect();
    let step = session.parser.advance_write();
    let changed = version != session.parser.dom().mutation_version();
    session.mutated |= changed;
    if changed {
        for node in Node::descendants(&host.document).filter(crate::engine::page::is_stylesheet) {
            if !previous.contains(&(node.id(), node.subtree_mutation_version()))
                && !session
                    .stylesheets
                    .iter()
                    .any(|sheet| sheet.id() == node.id())
            {
                session.stylesheets.push(node);
            }
        }
    }
    host.quirks_mode =
        session.parser.dom().quirks_mode.get() != html5ever::tree_builder::QuirksMode::NoQuirks;
    let node = if let ParserStep::Script(node) = &step {
        host.id_for(node)
    } else {
        0
    };
    let ids = if changed {
        let ids = host.register_parser_changes();
        let document = host.document.clone();
        host.record_mutation(Some(&document), MutationKind::Stylesheet);
        ids
    } else {
        String::new()
    };
    Ok(JsValue::Object(vec![
        ("node".into(), JsValue::from(node)),
        ("ids".into(), JsValue::from(ids)),
        ("changed".into(), JsValue::from(changed)),
        (
            "done".into(),
            JsValue::from(matches!(step, ParserStep::NeedInput | ParserStep::End)),
        ),
    ]))
}

fn prepare(host: &mut HostState, id: u32) -> JsResult<JsValue> {
    let Some(node) = host.node(id) else {
        return Ok(JsValue::Null);
    };
    if !host.is_connected(&node)
        || node
            .element()
            .is_none_or(|element| element.script_started.get())
    {
        return Ok(JsValue::Null);
    }
    let Some(session) = host.parser_write_session.as_mut() else {
        return Ok(JsValue::Null);
    };
    let ordinal = session.initial_count + session.prepared.len() + 1;
    if ordinal > crate::limits::MAX_PAGE_SCRIPTS {
        return Err(limit_error(
            "document.write exceeded the page script count limit".into(),
        ));
    }
    let (script, styles_pending) = (session.prepare)(node.clone(), ordinal, &session.stylesheets);
    let Some(script) = script else {
        return Ok(JsValue::Null);
    };
    let immediate = script.kind == ScriptKind::Classic && script.code.is_some() && !styles_pending;
    let value = if immediate {
        let code = script.code.as_ref().unwrap();
        if code.len() > MAX_SCRIPT_BYTES
            || session.script_bytes.saturating_add(code.len()) > session.remaining_script_bytes
        {
            return Err(limit_error(
                "document.write exceeded the page JavaScript byte limit".into(),
            ));
        }
        session.script_bytes += code.len();
        JsValue::Object(vec![
            ("code".into(), JsValue::from(code.clone())),
            ("url".into(), JsValue::from(script.source_url.clone())),
        ])
    } else {
        if script.blocks_first_paint {
            session.paused = true;
        }
        JsValue::Null
    };
    session.prepared.push((script, immediate));
    host.mark_script_started(&node);
    Ok(value)
}

fn limit_error(message: String) -> JsError {
    JsNativeError::range().with_message(message).into()
}

impl HostState {
    pub(super) fn register_parser_changes(&mut self) -> String {
        let document = self.document.clone();
        self.computed_styles = None;
        self.offset_parent_styles = None;
        self.layout_geometry_initialized = false;
        self.pending_layout_invalidation.record(
            &document,
            Some(&document),
            MutationKind::Stylesheet,
        );
        let new_elements = Node::descendants(&document)
            .filter(|node| node.element().is_some() && !self.node_ids.contains_key(&node.id()))
            .collect::<Vec<_>>();
        self.register_subtree(&document);
        new_elements
            .iter()
            .map(|node| self.id_for(node).to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}
