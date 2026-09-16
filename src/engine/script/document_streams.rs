//! HTML dynamic markup insertion: script-created input and in-place Document replacement.
//! https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html
use super::binding_helpers::argument_id;
use super::parser_writes::{ParserWriteSession, PrepareWrittenScript};
use super::*;
use crate::engine::dom::incremental::HtmlParser;

#[derive(Default)]
pub(super) struct DocumentStreams {
    pub(super) parsers: HashMap<NodeId, ParserWriteSession>,
    pub(super) prepare: Option<PrepareWrittenScript>,
    pub(super) replaced: bool,
    pub(super) generation: u64,
    pub(super) ignore_destructive: usize,
    pub(super) remaining_script_bytes: Option<usize>,
}

impl HostState {
    pub(super) fn begin_stream_script(&mut self, node: &NodeRef) -> (bool, bool, u64) {
        let external = self.script_is_external(node);
        self.document_streams.ignore_destructive += usize::from(external);
        let mut nested = false;
        if let Some(session) = self.document_streams.parsers.get_mut(&self.document.id())
            && session.blocked_on == Some(node.id())
        {
            session.nesting += 1;
            session.paused = false;
            nested = true;
        }
        (external, nested, self.document_streams.generation)
    }

    pub(super) fn end_stream_script(&mut self, (external, nested, generation): (bool, bool, u64)) {
        self.document_streams.ignore_destructive -= usize::from(external);
        if nested
            && generation == self.document_streams.generation
            && let Some(session) = self.document_streams.parsers.get_mut(&self.document.id())
        {
            session.nesting -= 1;
        }
    }

    pub(super) fn write_session(&mut self, id: u32) -> Option<&mut ParserWriteSession> {
        let target = self.node(id)?.id();
        if self.document_streams.parsers.contains_key(&target) {
            return self.document_streams.parsers.get_mut(&target);
        }
        self.parser_write_session
            .as_mut()
            .filter(|session| session.target.id() == target)
    }
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    host: &mut HostState,
) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "documentOpen" => open(host, argument_id(args, 1)),
        "ignoreDestructiveWrite" => JsValue::from(
            host.document_streams.ignore_destructive > 0 || host.navigation_url.is_some(),
        ),
        "documentClose" => {
            let closed = if let Some(session) = host.write_session(argument_id(args, 1)) {
                if session.script_created && !session.closed {
                    session.closed = true;
                    session
                        .parser
                        .append("", true)
                        .map_err(|error| JsNativeError::range().with_message(error))?;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            JsValue::from(closed)
        }
        "documentStreamNeedsClose" => JsValue::from(
            host.write_session(argument_id(args, 1))
                .is_some_and(|session| {
                    session.script_created && session.closed && !session.parser.ended()
                }),
        ),
        "documentStreamFinished" => {
            if host
                .node(argument_id(args, 1))
                .is_some_and(|node| node.id() == host.document.id())
            {
                host.document_load.stream_finished();
            }
            JsValue::undefined()
        }
        _ => return Ok(None),
    }))
}

fn open(host: &mut HostState, id: u32) -> JsValue {
    let Some(document) = host.node(id) else {
        return JsValue::Null;
    };
    if host.navigation_url.is_some()
        || host.write_session(id).is_some_and(|session| {
            session.nesting > 0 || (!session.script_created && session.insertion_point)
        })
    {
        return JsValue::Null;
    }
    let primary = document.id() == host.document.id();
    // Replacement does not create a fresh realm or reset its aggregate JS budget.
    let (script_bytes, remaining_script_bytes) = host
        .document_streams
        .parsers
        .get(&document.id())
        .map(|session| (session.script_bytes, session.remaining_script_bytes))
        .unwrap_or((
            0,
            host.document_streams
                .remaining_script_bytes
                .unwrap_or(MAX_PAGE_SCRIPT_BYTES),
        ));
    let mut nodes = Vec::new();
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        nodes.push(host.id_for(&node).to_string());
        stack.extend(node.children.borrow().iter().cloned());
        stack.extend(node.shadow_root());
    }
    Node::set_text_content(&document, "");
    let prepare = host.document_streams.prepare.clone().unwrap_or_else(|| {
        let url = host.document_url.clone();
        Rc::new(RefCell::new(move |node, ordinal, _: &[NodeRef]| {
            (
                crate::engine::page::prepare_written_script(node, &url, ordinal),
                false,
            )
        }))
    });
    host.document_streams.parsers.insert(
        document.id(),
        ParserWriteSession {
            target: document.clone(),
            script_created: true,
            closed: false,
            nesting: 0,
            parser: HtmlParser::for_document(document.clone()),
            prepare,
            prepared: Vec::new(),
            stylesheets: Vec::new(),
            mutated: true,
            initial_count: 0,
            root: document.id(),
            insertion_point: true,
            paused: false,
            blocked_on: None,
            script_bytes,
            remaining_script_bytes,
        },
    );
    if primary {
        host.document_streams.replaced = true;
        host.document_streams.generation += 1;
        host.document_load = Default::default();
        host.quirks_mode = false;
        host.stylesheet_sources.clear();
        host.pointer_path.clear();
        host.pending_dynamic_scripts.clear();
    }
    host.register_parser_changes_in(&document);
    host.record_mutation(Some(&document), MutationKind::Stylesheet);
    JsValue::from(nodes.join(","))
}
