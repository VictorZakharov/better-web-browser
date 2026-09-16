//! Synchronous parser operations. Host borrows end before author JS or CE reactions run.
use super::binding_helpers::{argument_id, argument_string};
use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};
use crate::engine::page::PageScript;

pub(crate) type PrepareWrittenScript =
    Rc<RefCell<dyn FnMut(NodeRef, usize, &[NodeRef]) -> (Option<PageScript>, bool)>>;

pub(super) struct ParserWriteSession {
    pub(super) target: NodeRef,
    pub(super) script_created: bool,
    pub(super) closed: bool,
    pub(super) nesting: usize,
    pub(super) parser: HtmlParser,
    pub(super) prepare: PrepareWrittenScript,
    pub(super) prepared: Vec<(PageScript, bool)>,
    pub(super) stylesheets: Vec<NodeRef>,
    pub(super) mutated: bool,
    pub(super) initial_count: usize,
    pub(super) root: NodeId,
    pub(super) insertion_point: bool,
    pub(super) paused: bool,
    pub(super) blocked_on: Option<NodeId>,
    pub(super) script_bytes: usize,
    pub(super) remaining_script_bytes: usize,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    host: &mut HostState,
) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "observeParserMutations" => {
            if let Some(node) = host.node(argument_id(args, 1)) {
                node.observe_parser_mutations(argument_id(args, 2) != 0);
                if argument_id(args, 3) != 0 {
                    node.observe_parser_old_text(argument_id(args, 2) != 0);
                }
            }
            JsValue::undefined()
        }
        "parserWriteBegin" => {
            let id = argument_id(args, 1);
            if host
                .write_session(id)
                .is_none_or(|session| !session.insertion_point || session.parser.ended())
            {
                return Ok(Some(JsValue::from(false)));
            }
            let text = argument_string(args, 2)?;
            host.ensure_node_capacity(super::mutation_host::estimated_markup_nodes(&text))?;
            let session = host.write_session(id).unwrap();
            session.parser.begin_write(text).map_err(limit_error)?;
            JsValue::from(true)
        }
        "parserWriteStep" => step(host, argument_id(args, 1), false)?,
        "parserStreamStep" => step(host, argument_id(args, 1), true)?,
        "parserWritePrepare" => prepare(host, argument_id(args, 1), argument_id(args, 2))?,
        "parserWriteEnd" => {
            if let Some(session) = host.write_session(argument_id(args, 1)) {
                session.parser.end_write();
            }
            JsValue::undefined()
        }
        "parserWriteExecuted" => {
            host.executed += 1;
            JsValue::undefined()
        }
        "parserScriptEnter" | "parserScriptLeave" => {
            if let Some(session) = host.write_session(argument_id(args, 1)) {
                if operation == "parserScriptEnter" {
                    session.nesting += 1;
                } else {
                    session.nesting = session.nesting.saturating_sub(1);
                }
            }
            JsValue::undefined()
        }
        _ => return Ok(None),
    }))
}

fn step(host: &mut HostState, id: u32, stream: bool) -> JsResult<JsValue> {
    if host.navigation_url.is_some() {
        return Ok(JsValue::Null);
    }
    let Some(session) = host.write_session(id) else {
        return Ok(JsValue::Null);
    };
    if session.paused || (stream && (session.nesting > 0 || session.parser.writing())) {
        return Ok(JsValue::Null);
    }
    let version = session.parser.dom().mutation_version();
    let target = session.target.clone();
    let previous: Vec<_> = Node::descendants(&target)
        .filter(crate::engine::page::is_stylesheet)
        .map(|node| (node.id(), node.subtree_mutation_version()))
        .collect();
    let step = if stream {
        session.parser.advance()
    } else {
        session.parser.advance_write()
    };
    let changed = version != session.parser.dom().mutation_version();
    session.mutated |= changed;
    if changed {
        for node in Node::descendants(&target).filter(crate::engine::page::is_stylesheet) {
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
    let quirks =
        session.parser.dom().quirks_mode.get() != html5ever::tree_builder::QuirksMode::NoQuirks;
    let ended = session.script_created && session.parser.ended();
    let records = session.parser.dom().take_parser_mutations();
    if target.id() == host.document.id() {
        host.quirks_mode = quirks;
    }
    let node = if let ParserStep::Script(node) = &step {
        host.id_for(node)
    } else {
        0
    };
    let ids = if changed {
        let ids = host.register_parser_changes_in(&target);
        host.record_mutation(Some(&target), MutationKind::Stylesheet);
        ids
    } else {
        String::new()
    };
    Ok(JsValue::Object(vec![
        ("mutations".into(), parser_mutation_records(host, records)),
        ("node".into(), JsValue::from(node)),
        ("ids".into(), JsValue::from(ids)),
        ("changed".into(), JsValue::from(changed)),
        ("ended".into(), JsValue::from(ended)),
        (
            "done".into(),
            JsValue::from(matches!(step, ParserStep::NeedInput | ParserStep::End)),
        ),
    ]))
}

fn prepare(host: &mut HostState, target: u32, id: u32) -> JsResult<JsValue> {
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
    let Some(session) = host.write_session(target) else {
        return Ok(JsValue::Null);
    };
    let ordinal = session.initial_count + session.prepared.len() + 1;
    if ordinal > crate::limits::MAX_PAGE_SCRIPTS {
        return Err(limit_error(
            "document.write exceeded the page script count limit".into(),
        ));
    }
    let (script, styles_pending) =
        (session.prepare.borrow_mut())(node.clone(), ordinal, &session.stylesheets);
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
            session.blocked_on = Some(node.id());
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
        self.register_parser_changes_in(&document)
    }

    pub(super) fn register_parser_changes_in(&mut self, document: &NodeRef) -> String {
        self.computed_styles = None;
        self.offset_parent_styles = None;
        self.layout_geometry_initialized = false;
        self.pending_layout_invalidation
            .record(document, Some(document), MutationKind::Stylesheet);
        let new_elements = Node::descendants(document)
            .filter(|node| node.element().is_some() && !self.node_ids.contains_key(&node.id()))
            .collect::<Vec<_>>();
        self.register_subtree(document);
        new_elements
            .iter()
            .map(|node| self.id_for(node).to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub(super) fn parser_mutation_records(
    host: &mut HostState,
    records: Vec<crate::engine::dom::document::parser_mutations::ParserMutation>,
) -> JsValue {
    JsValue::Array(
        records
            .into_iter()
            .map(|record| {
                let mut ids = |nodes: Vec<NodeRef>| {
                    JsValue::Array(
                        nodes
                            .iter()
                            .map(|node| JsValue::from(host.id_for(node)))
                            .collect(),
                    )
                };
                let ancestors = ids(record.ancestors);
                let added = ids(record.added);
                let removed = ids(record.removed);
                JsValue::Object(vec![
                    ("target".into(), JsValue::from(host.id_for(&record.target))),
                    ("type".into(), JsValue::from(record.kind.to_owned())),
                    ("ancestors".into(), ancestors),
                    ("added".into(), added),
                    ("removed".into(), removed),
                    (
                        "previous".into(),
                        JsValue::from(record.previous.as_ref().map_or(0, |n| host.id_for(n))),
                    ),
                    (
                        "next".into(),
                        JsValue::from(record.next.as_ref().map_or(0, |n| host.id_for(n))),
                    ),
                    (
                        "name".into(),
                        record
                            .attribute
                            .as_ref()
                            .map_or(JsValue::Null, |a| JsValue::from(a.0.clone())),
                    ),
                    (
                        "namespace".into(),
                        record
                            .attribute
                            .as_ref()
                            .filter(|a| !a.1.is_empty())
                            .map_or(JsValue::Null, |a| JsValue::from(a.1.clone())),
                    ),
                    (
                        "oldValue".into(),
                        record.old_value.map_or(JsValue::Null, JsValue::from),
                    ),
                ])
            })
            .collect(),
    )
}
