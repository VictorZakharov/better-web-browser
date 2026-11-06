//! Viewport changes and display-only cadence updates for a retained document.
//
// A classic scrollbar takes its width from the layout viewport, and the browser shows it only
// after a presentation overflows. A long document was therefore laid out completely at the width
// it was about to lose, and every result retained from that layout depends on the viewport: on a
// Wikipedia article that first layout was about 10% of the renderer's work up to its first useful
// frame. A document this long is laid out at the width it will have with the scrollbar instead,
// and the browser's scrollbar then changes nothing. If it does fit, it is laid out again at once,
// before anything is presented, which costs little because it is short.
use super::*;

/// Nodes from which a document is assumed to overflow. A guess is only worth making when laying
/// the document out twice would be expensive, and shorter ones can keep the observed behavior.
const LONG_DOCUMENT_NODES: usize = 1024;

/// A guess that the document overflows, made while the browser shows no scrollbar.
#[derive(Debug, Default)]
pub(super) struct OverflowGuess {
    /// The width the browser reported, while layout uses its `overflow_width` instead.
    received_width: Option<f32>,
    /// The guess was refuted or overtaken by the browser's scrollbar.
    settled: bool,
}

impl DocumentRuntime {
    /// Lays a long document out as if its scrollbar already showed.
    pub(super) fn guess_viewport_overflow(&mut self) {
        if self.overflow_guess.settled || self.overflow_guess.received_width.is_some() {
            return;
        }
        if self.viewport.width <= self.viewport.overflow_width {
            self.overflow_guess.settled = true;
            return;
        }
        let nodes = crate::engine::dom::Node::descendants(&self.page.dom.document)
            .take(LONG_DOCUMENT_NODES)
            .count();
        if nodes == LONG_DOCUMENT_NODES {
            self.overflow_guess.received_width = Some(self.viewport.width);
            self.set_layout_width(self.viewport.overflow_width);
        }
    }

    /// Whether the layout just computed refutes the guess, which has then been withdrawn and
    /// calls for another layout. The browser shows a scrollbar for content over a physical pixel
    /// taller than the viewport, so anything less must not wait for one.
    pub(super) fn withdraw_refuted_overflow_guess(&mut self) -> bool {
        let Some(width) = self.overflow_guess.received_width else {
            return false;
        };
        let pixel = 96.0 / self.viewport.dpi as f32;
        if self.layout.content_height > self.viewport.height + pixel {
            return false;
        }
        self.overflow_guess = OverflowGuess {
            received_width: None,
            settled: true,
        };
        self.set_layout_width(width);
        true
    }

    /// `viewport` from the browser with the standing guess applied.
    fn with_overflow_guess(
        &mut self,
        mut viewport: crate::renderer_protocol::PresentedViewport,
    ) -> crate::renderer_protocol::PresentedViewport {
        if viewport.width <= viewport.overflow_width {
            self.overflow_guess = OverflowGuess {
                received_width: None,
                settled: true,
            };
        } else if self.overflow_guess.received_width.is_some() {
            self.overflow_guess.received_width = Some(viewport.width);
            viewport.width = viewport.overflow_width;
        }
        viewport
    }

    fn set_layout_width(&mut self, width: f32) {
        self.viewport.width = width;
        let height = self.viewport.height;
        self.page.set_layout_viewport(width, height);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_layout_viewport(width, height);
        }
    }

    pub(in crate::renderer_process::child) fn resize(
        &mut self,
        viewport: crate::renderer_protocol::PresentedViewport,
        connection: &mut ChildConnection,
    ) -> Result<AdvanceResult, String> {
        let viewport = viewport.validate().map_err(|error| error.to_string())?;
        let viewport = self.with_overflow_guess(viewport);
        let cadence_only = viewport.refresh_rate != self.viewport.refresh_rate
            && crate::renderer_protocol::PresentedViewport {
                refresh_rate: viewport.refresh_rate,
                ..self.viewport
            } == viewport;
        let previous = std::mem::replace(&mut self.viewport, viewport);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_rendering_environment(
                viewport.refresh_rate,
                self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Active,
            );
        }
        // A monitor move can change cadence without changing CSS geometry. It
        // must not dispatch resize or reset the game's drawing buffer.
        if cadence_only {
            return Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: false,
                runtime: runtime_report(
                    ScriptOutcome::default(),
                    self.script_runtime.is_some(),
                    self.media_runtime_report(),
                ),
                load: PageLoadReport::default(),
                next_timer_micros: self.next_timer_micros(),
            })));
        }
        self.apply_media_environment(viewport);
        self.text.borrow_mut().set_dpi(viewport.dpi);
        self.sync_script_layout_page();
        let mut outcome = self
            .dispatch_user_input(crate::engine::UserInputEvent::Viewport {
                width: viewport.style_width,
                height: viewport.height,
                layout_width: viewport.width,
                layout_height: viewport.height,
                scale: viewport.dpi as f32 / 96.0,
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        // The cascade sees the style width, never the layout width: a classic scrollbar that
        // appears or disappears narrows layout without changing media queries or viewport
        // units. Keep cached styles then, and refresh only what resize handlers invalidated.
        let invalidation = if same_style_environment(&previous, &viewport) {
            outcome.invalidation.clone()
        } else {
            crate::engine::invalidation::RenderInvalidation::full(self.page.dom.document.id())
        };
        let style = self.page.refresh_resources_after_invalidation_for_viewport(
            viewport.style_width,
            viewport.height,
            &invalidation,
        );
        self.start_presentational_preloads(connection)?;
        let started = Instant::now();
        self.rebuild_layout();
        let load = self.finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        self.presentation(outcome, style, load, connection, None)
    }
}

fn same_style_environment(
    previous: &crate::renderer_protocol::PresentedViewport,
    next: &crate::renderer_protocol::PresentedViewport,
) -> bool {
    previous.style_width == next.style_width
        && previous.height == next.height
        && previous.dpi == next.dpi
        && previous.prefers_dark_color_scheme == next.prefers_dark_color_scheme
}
