//! Nonblocking work selection and rendering checkpoints for a retained document.
use super::*;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn next_timer_micros(&mut self) -> Option<u64> {
        self.collect_document_stream_changes();
        if self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Frozen {
            return None;
        }
        self.publish_document_load_readiness(
            self.resource_render_pending
                || self.resource_event_pending
                || self.pending_async_outcome.render_requested,
        );
        if self.has_pending_geometry_observers()
            || self.resource_render_pending
            || self.resource_event_pending
            || (self.rendering.dirty && !self.rendering_is_blocked())
        {
            return Some(0);
        }
        let runtime_timer = self
            .script_runtime
            .as_mut()
            .and_then(ScriptRuntime::next_timer_delay)
            .map(|delay| delay.as_micros().min(u64::MAX as u128) as u64);
        let runtime_timer = if self.has_post_load_work() {
            Some(runtime_timer.unwrap_or(0).min(10_000))
        } else {
            runtime_timer
        };
        [
            runtime_timer,
            self.media_timer_micros(),
            self.rendering_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn has_post_load_work(&self) -> bool {
        !self.pending_fetches.is_empty()
            || !self.pending_worker_actions.is_empty()
            || self.workers.has_work()
            || self
                .script_runtime
                .as_ref()
                .is_some_and(ScriptRuntime::has_runnable_dynamic_scripts)
            || self
                .parser_scripts
                .has_ready_with_styles(!self.parser_stylesheets_pending())
            || self.parser_runnable()
    }

    pub(in crate::renderer_process::child) fn advance(
        &mut self,
        elapsed: Duration,
        max_callbacks: u32,
        connection: &mut ChildConnection,
    ) -> Result<AdvanceResult, String> {
        if self.lifecycle == crate::renderer_protocol::DocumentLifecycle::Frozen {
            return Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: true,
                runtime: runtime_report(
                    ScriptOutcome::default(),
                    self.script_runtime.is_some(),
                    self.media_runtime_report(),
                ),
                load: PageLoadReport::default(),
                next_timer_micros: None,
            })));
        }
        self.resource_event_pending = false;
        self.collect_document_stream_changes();
        self.prepare_document_streams();
        let mut outcome = std::mem::take(&mut self.pending_async_outcome);
        // IntersectionObserver callbacks are tasks, unlike ResizeObserver's before-paint loop.
        if !self.rendering_is_blocked()
            && let Some(runtime) = self.script_runtime.as_mut()
            && runtime.has_intersection_task()
        {
            merge_outcome(
                &mut outcome,
                runtime.notify_intersection_observers(),
                self.page.dom.document.id(),
            );
        }
        let mut resources_changed = std::mem::take(&mut self.resource_render_pending);
        let mut resource_style_changed = std::mem::take(&mut self.resource_style_refresh_pending);
        if let Some(changes) = self.finish_ready_resource_preloads(connection)? {
            resources_changed |= changes.render;
            resource_style_changed |= changes.style;
        }
        merge_outcome(
            &mut outcome,
            std::mem::take(&mut self.pending_async_outcome),
            self.page.dom.document.id(),
        );
        if resources_changed {
            self.text.borrow_mut().register_web_fonts(&self.page.fonts);
            self.sync_script_layout_page();
        }
        let mut script_time = Duration::ZERO;
        let async_script_started = Instant::now();
        connection.report_renderer_task_stage(format!(
            "checking deferred scripts for {}",
            self.page.source_url
        ))?;
        self.advance_parser(connection, &mut outcome)?;
        if self.encoding_restart_pending() {
            connection.send_state_mutations(self.id, &mut outcome)?;
            return Ok(AdvanceResult::EncodingRestart);
        }
        self.execute_pending_parser_script(connection, &mut outcome)?;
        script_time += async_script_started.elapsed();
        self.start_pending_fetches(connection)?;
        let document_url = self.page.source_url.clone();
        let document_root = self.page.dom.document.id();
        let worker_actions = std::mem::take(&mut self.pending_worker_actions);
        connection
            .report_renderer_task_stage(format!("driving workers for {}", self.page.source_url))?;
        self.workers.drive(
            worker_actions,
            workers::WorkerDriveContext {
                connection,
                document: self.id,
                document_url: &document_url,
                runtime: &mut self.script_runtime,
                document_root,
                outcome: &mut outcome,
            },
        )?;

        self.start_dynamic_script_fetches(connection)?;
        self.finish_ready_dynamic_scripts(connection)?;

        // Earlier script/resource callbacks may have introduced new load-delaying resources.
        // Discover them at this checkpoint before permitting the later window-load task.
        self.publish_document_load_readiness(resources_changed || outcome.render_requested);
        if let Some(runtime) = self.script_runtime.as_mut() {
            connection.report_renderer_task_stage(format!(
                "settling timers and promise jobs for {}",
                self.page.source_url
            ))?;
            let timer_started = Instant::now();
            let callback_limit = max_callbacks.min(MAX_POST_LOAD_TIMER_CALLBACKS as u32) as usize;
            let timed = if runtime.has_ready_dynamic_scripts() && !runtime.has_ready_document_task()
            {
                advance_dynamic_script_slice(runtime, document_root, elapsed, callback_limit)
            } else {
                let document_url = self.page.source_url.clone();
                let mut reporter = |stage: &str| {
                    let _ = connection
                        .report_renderer_task_stage(format!("{stage} for {document_url}"));
                };
                runtime.advance_time_with_loader_and_stage_reporter(
                    elapsed,
                    callback_limit,
                    None,
                    Some(&mut reporter),
                )
            };
            script_time += timer_started.elapsed();
            merge_outcome(&mut outcome, timed, self.page.dom.document.id());
        }
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.start_dynamic_script_fetches(connection)?;
        let worker_actions = std::mem::take(&mut outcome.worker_actions);
        connection.report_renderer_task_stage(format!(
            "driving post-script workers for {}",
            self.page.source_url
        ))?;
        self.workers.drive(
            worker_actions,
            workers::WorkerDriveContext {
                connection,
                document: self.id,
                document_url: &document_url,
                runtime: &mut self.script_runtime,
                document_root,
                outcome: &mut outcome,
            },
        )?;
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.apply_media_actions(&mut outcome, connection)?;
        let media_changed = self.advance_media(elapsed, connection, &mut outcome)?;
        // Media events execute author script too. Admit their fetch/worker/media
        // commands before publishing the report, which only retains diagnostics.
        self.admit_user_input_outcome(&mut outcome, connection)?;

        // Script execution, console output, storage/cookie traffic, and worker progress are not
        // visual invalidations. Sending a complete display-list snapshot for those tasks made
        // timer-heavy pages continuously serialize, install, and repaint an unchanged document.
        let style_started = Instant::now();
        let style = if outcome.render_requested {
            connection.report_renderer_task_stage(format!(
                "refreshing styles for {}",
                self.page.source_url
            ))?;
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &outcome.invalidation,
            )
        } else if resource_style_changed {
            self.page
                .refresh_resources_for_viewport(self.viewport.style_width, self.viewport.height)
        } else {
            StyleRefreshStats::default()
        };
        let style_time = style_started.elapsed();
        let mut needs_present = resources_changed
            || media_changed
            || outcome.render_requested
            || (self.rendering.dirty && !self.rendering_is_blocked());
        // A rendering checkpoint can discover resources in newly-created shadow trees. Start the
        // browser fetch now, but do not wait inside this renderer task. The response path installs
        // the completed batch and presents the resulting layout without blocking heartbeats.
        self.start_presentational_preloads(connection)?;
        let layout_started = Instant::now();
        // Reuse only box geometry. A hidden DOM change can still update the document title
        // or an accessibility name, so keep the regular presentation/metadata path.
        if resources_changed
            || (self.rendering.dirty && !self.rendering_is_blocked())
            || (outcome.render_requested
                && !self
                    .page
                    .invalidation_is_nonrendered(&outcome.invalidation, &style))
        {
            connection.report_renderer_task_stage(format!(
                "rebuilding layout for {}",
                self.page.source_url
            ))?;
            self.rebuild_layout();
        }
        needs_present |= self.deliver_geometry_observers(&mut outcome, connection)?;
        let layout_time = layout_started.elapsed();
        if needs_present && !self.diagnostic_selectors.is_empty() {
            outcome.diagnostics.push(format!(
                "render checkpoint: style/resources {:.3} ms (elements {:.3}, pseudos {:.3}), layout {:.3} ms; styles {}/{} changed, full rebuild {}, dirty roots {} {:?}, removed styles {}, local removals {}",
                style_time.as_secs_f64() * 1000.0,
                style.element_style_time.as_secs_f64() * 1000.0,
                style.pseudo_style_time.as_secs_f64() * 1000.0,
                layout_time.as_secs_f64() * 1000.0,
                style.changed_styles,
                style.recomputed_styles,
                style.full_rebuild,
                outcome.invalidation.roots.len(),
                outcome.invalidation.roots.iter().take(8).map(|id| self.page.dom.find_node(*id).and_then(|n| n.tag_name().map(str::to_owned))).collect::<Vec<_>>(),
                style.removed_styles,
                outcome.invalidation.removals_are_local,
            ));
        }
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            script_micros: micros(script_time),
            style_micros: micros(style_time),
            layout_micros: micros(layout_time),
            ..PageLoadReport::default()
        });
        if needs_present {
            self.presentation_after_observers(outcome, style, load)
                .map(|mut presentation| {
                    match &mut presentation {
                        AdvanceResult::Presentation(value) => value.clock_advanced = true,
                        AdvanceResult::Runtime(value) => value.clock_advanced = true,
                        AdvanceResult::EncodingRestart => {}
                    }
                    presentation
                })
        } else {
            let next_timer_micros = self.next_timer_micros();
            Ok(AdvanceResult::Runtime(Box::new(RendererRuntimeUpdate {
                document: self.id,
                clock_advanced: true,
                runtime: runtime_report(
                    outcome,
                    self.script_runtime.is_some(),
                    self.media_runtime_report(),
                ),
                load,
                next_timer_micros,
            })))
        }
    }
}
