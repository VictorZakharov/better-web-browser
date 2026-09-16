//! Render blocking is not parser or event-loop blocking. Only runtime updates cross IPC until
//! the document's initial matching head stylesheets settle (including errors/removal).
//! https://html.spec.whatwg.org/multipage/dom.html#render-blocking-mechanism
use super::*;
use crate::engine::dom::{Node, NodeId, NodeRef};

pub(super) struct RenderBlocking {
    links: HashMap<NodeId, NodeRef>,
    scripts: HashMap<NodeId, NodeRef>,
    started: Instant,
    pub(super) dirty: bool,
}
impl Default for RenderBlocking {
    fn default() -> Self {
        Self {
            links: HashMap::new(),
            scripts: HashMap::new(),
            started: Instant::now(),
            dirty: false,
        }
    }
}

impl DocumentRuntime {
    pub(super) fn head_links(&self) -> Vec<NodeRef> {
        self.page
            .dom
            .elements_named("head")
            .next()
            .map_or_else(Vec::new, |head| {
                Node::descendants(&head)
                    .filter(crate::engine::page::is_stylesheet)
                    .collect()
            })
    }
    pub(super) fn stylesheet_nodes(&self) -> Vec<NodeRef> {
        Node::descendants(&self.page.dom.document)
            .filter(crate::engine::page::is_stylesheet)
            .collect()
    }
    pub(super) fn record_parser_stylesheets(&mut self, previous: &[(NodeId, u64)]) {
        let written = self
            .stylesheet_nodes()
            .into_iter()
            .filter(|node| !previous.contains(&(node.id(), node.subtree_mutation_version())))
            .collect();
        self.record_written_stylesheets(written);
    }
    pub(super) fn record_written_stylesheets(&mut self, nodes: Vec<NodeRef>) {
        let head = self.head_links();
        for node in nodes {
            if Node::tree_root(&node).id() == self.page.dom.document.id()
                && self.stylesheet_pending(&node)
            {
                // HTML's script-blocking set includes parser-created body links and style
                // imports, unlike implicit head-only first-presentation blocking.
                self.rendering.scripts.insert(node.id(), node.clone());
                if head.iter().any(|candidate| candidate.id() == node.id()) {
                    self.rendering.links.insert(node.id(), node);
                }
            }
        }
    }
    pub(super) fn update_render_blockers(&mut self) {
        // Admission and release are lifecycle operations. A link that has finished, been
        // disabled, or disconnected must not become a blocker again after body insertion.
        let head_links = self.head_links();
        let scripts = std::mem::take(&mut self.rendering.scripts);
        self.rendering.scripts = scripts
            .into_iter()
            .filter(|(_, node)| {
                Node::tree_root(node).id() == self.page.dom.document.id()
                    && self.stylesheet_pending(node)
            })
            .collect();
        let previous = std::mem::take(&mut self.rendering.links);
        self.rendering.links = previous
            .into_iter()
            .filter(|(id, node)| {
                head_links.iter().any(|connected| connected.id() == *id)
                    && self.stylesheet_pending(node)
            })
            .collect();
        if self.page.dom.elements_named("body").next().is_none() {
            for node in head_links {
                if node
                    .attr("blocking")
                    .is_some_and(|value| value.split_ascii_whitespace().any(|v| v == "render"))
                    && self.stylesheet_pending(&node)
                {
                    self.rendering.links.insert(node.id(), node);
                }
            }
        }
    }
    pub(super) fn rendering_is_blocked(&self) -> bool {
        if self.rendering.started.elapsed() >= Duration::from_secs(30) {
            return false;
        }
        let no_body = self.page.dom.elements_named("body").next().is_none();
        if no_body && self.parser.is_some() {
            return true;
        }
        self.head_links().iter().any(|node| {
            self.rendering.links.contains_key(&node.id()) && self.stylesheet_pending(node)
        })
    }
    fn stylesheet_pending(&self, node: &NodeRef) -> bool {
        stylesheet_pending(
            &self.page,
            node,
            &self.loaded_resources,
            &self.page.resources,
        )
    }
    pub(super) fn parser_stylesheets_pending(&self) -> bool {
        self.rendering.scripts.values().any(|node| {
            Node::tree_root(node).id() == self.page.dom.document.id()
                && self.stylesheet_pending(node)
        })
    }
    pub(super) fn rendering_deadline(&self) -> Option<u64> {
        self.rendering_is_blocked().then(|| {
            micros(Duration::from_secs(30).saturating_sub(self.rendering.started.elapsed()))
        })
    }
    pub(super) fn blocked_render_update(
        &mut self,
        outcome: ScriptOutcome,
        load: PageLoadReport,
    ) -> AdvanceResult {
        self.rendering.dirty = true;
        let next_timer_micros = self.next_timer_micros();
        AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
            document: self.id,
            clock_advanced: false,
            load,
            next_timer_micros,
            runtime: runtime_report(
                outcome,
                self.script_runtime.is_some(),
                self.media_runtime_report(),
            ),
        }))
    }
}

// Both normal parser checkpoints and synchronous written scripts use the same stylesheet
// applicability, response, and admission policy. Unadmitted resources cannot block forever.
pub(super) fn stylesheet_pending(
    page: &Page,
    node: &NodeRef,
    loaded: &HashSet<PageResource>,
    admitted: &[PageResource],
) -> bool {
    page.stylesheet_applies(node)
        && page.stylesheet_dependencies(node).urls.iter().any(|url| {
            let resource = PageResource::Stylesheet { url: url.clone() };
            !loaded.contains(&resource)
                && (admitted.contains(&resource)
                    || admitted
                        .iter()
                        .filter(|r| matches!(r, PageResource::Stylesheet { .. }))
                        .count()
                        < crate::limits::MAX_STYLESHEETS)
        })
}
