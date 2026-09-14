//! Render blocking is not parser or event-loop blocking. Only runtime updates cross IPC until
//! the document's initial matching head stylesheets settle (including errors/removal).
//! https://html.spec.whatwg.org/multipage/dom.html#render-blocking-mechanism
use super::*;
use crate::engine::dom::{Node, NodeId, NodeRef};

pub(super) struct RenderBlocking {
    links: HashMap<NodeId, NodeRef>,
    started: Instant,
    pub(super) dirty: bool,
}
impl Default for RenderBlocking {
    fn default() -> Self {
        Self {
            links: HashMap::new(),
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
                    .filter(|node| node.tag_name() == Some("link"))
                    .collect()
            })
    }
    pub(super) fn record_parser_stylesheets(&mut self, previous: &[NodeRef]) {
        for node in self.head_links() {
            if !previous.iter().any(|old| old.id() == node.id()) && self.stylesheet_pending(&node) {
                self.rendering.links.insert(node.id(), node);
            }
        }
    }
    pub(super) fn update_render_blockers(&mut self) {
        // Admission and release are lifecycle operations. A link that has finished, been
        // disabled, or disconnected must not become a blocker again after body insertion.
        let head_links = self.head_links();
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
        let rel = node.attr("rel").unwrap_or_default();
        if !rel
            .split_ascii_whitespace()
            .any(|v| v.eq_ignore_ascii_case("stylesheet"))
            || rel
                .split_ascii_whitespace()
                .any(|v| v.eq_ignore_ascii_case("alternate"))
            || node.attr("disabled").is_some()
            || node
                .attr("type")
                .is_some_and(|v| !v.is_empty() && !v.eq_ignore_ascii_case("text/css"))
        {
            return false;
        }
        if !crate::engine::css::media::media_matches_for_environment(
            &node.attr("media").unwrap_or_default(),
            MediaEnvironment::new(
                self.viewport.style_width,
                self.viewport.height,
                self.viewport.dpi as f32 / 96.0,
                self.prefers_dark_color_scheme,
            ),
        ) {
            return false;
        }
        let Some(url) = node
            .attr("href")
            .filter(|v| !v.is_empty())
            .and_then(|value| self.page.resolve_resource_url(&value))
        else {
            return false;
        };
        !self
            .loaded_resources
            .contains(&PageResource::Stylesheet { url })
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
