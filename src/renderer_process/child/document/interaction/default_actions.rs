//! Renderer-owned link and form default actions after cancelable DOM dispatch.

use super::*;
mod checkable;

impl DocumentRuntime {
    pub(super) fn pointer_default_action(
        &mut self,
        target: Option<&HitTarget>,
        input: PointerInput,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
        let Some(target) = target else {
            return Ok(None);
        };
        if self.script_runtime.is_none() && input.button == PointerButton::Primary {
            checkable::activate(&target.node, &self.page.dom.document, outcome);
        }
        // The DOM activation target, not the presence of a text paint command, owns a link.
        // This also covers block anchors, images, padding and nested inline descendants.
        let link = self.link_for_node(&target.node);
        if let Some(url) = link {
            let disposition = navigation_disposition(input);
            let current = self
                .script_runtime
                .as_ref()
                .map(ScriptRuntime::document_url)
                .unwrap_or_else(|| self.page.source_url.clone());
            if disposition == NavigationDisposition::CurrentTab
                && crate::engine::fragment_navigation::is_same_document(&current, &url)
            {
                if self.script_runtime.is_some() {
                    let fragment =
                        self.dispatch_user_input(UserInputEvent::FragmentNavigation {
                            url: url.clone(),
                        })?;
                    merge_outcome(outcome, fragment.outcome, self.page.dom.document.id());
                } else {
                    outcome.viewport_scroll_y =
                        crate::engine::fragment_navigation::scroll_to_fragment(
                            &self.page.dom.document,
                            &url,
                            &self.layout.node_bounds,
                            &self.layout.scroll_boxes,
                            self.layout.content_height - self.viewport.height,
                        );
                    if let Some(y) = outcome.viewport_scroll_y {
                        self.page.dom.document.scroll_offset.set((0.0, y));
                    }
                    if current != url {
                        outcome
                            .history_actions
                            .push(crate::engine::script::ScriptHistoryAction {
                                url: url.clone(),
                                replace: false,
                            });
                    }
                    outcome.render_requested = true;
                }
                self.reader.source_url.clone_from(&url);
                self.page.source_url = url;
                return Ok(None);
            }
            return Ok(Some((url, disposition)));
        }
        let Some(control) = target.control.as_ref() else {
            return Ok(None);
        };
        // Script-backed activation has already used the DOM submission algorithm.
        if self.script_runtime.is_some() {
            return Ok(None);
        }
        if control.kind == ControlKind::Reset {
            let Some(form_id) = control.form_id else {
                return Ok(None);
            };
            let Some(form_node) = self.page.dom.find_node(form_id) else {
                return Ok(None);
            };
            let reset = self.dispatch_user_input(UserInputEvent::Simple {
                target: form_node.clone(),
                event_type: "reset",
                bubbles: true,
                cancelable: true,
            })?;
            merge_outcome(outcome, reset.outcome, self.page.dom.document.id());
            if self.script_runtime.is_none() && reset.default_allowed {
                // No listeners exist scriptless; the event dispatch above is a
                // no-op, so reset authoritative state directly on success.
                crate::engine::dom::Node::reset_owned_controls(&form_node, &self.page.dom.document);
                outcome.render_requested = true;
            }
            return Ok(None);
        }
        if !matches!(control.kind, ControlKind::Submit) {
            return Ok(None);
        }
        let Some(form_id) = control.form_id else {
            return Ok(None);
        };
        self.submit_form(form_id, Some(control.node_id), outcome)
    }

    pub(super) fn keyboard_default_action(
        &mut self,
        target: Option<NodeId>,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
        if self.script_runtime.is_some() {
            return Ok(None);
        }
        let Some(control) = target.and_then(|target| {
            self.layout.items.iter().find_map(|item| match item {
                DisplayItem::Control(control) if control.node_id == target => Some(control.clone()),
                _ => None,
            })
        }) else {
            return Ok(None);
        };
        if control.kind == ControlKind::TextArea {
            return Ok(None);
        }
        let Some(form_id) = control.form_id else {
            return Ok(None);
        };
        let submitter = self.layout.items.iter().find_map(|item| match item {
            DisplayItem::Control(control)
                if control.form_id == Some(form_id) && control.kind == ControlKind::Submit =>
            {
                Some(control.node_id)
            }
            _ => None,
        });
        self.submit_form(form_id, submitter, outcome)
    }

    fn submit_form(
        &mut self,
        form_id: NodeId,
        submitter: Option<NodeId>,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
        let Some(form_node) = self.page.dom.find_node(form_id) else {
            return Ok(None);
        };
        let submit = self.dispatch_user_input(UserInputEvent::Simple {
            target: form_node.clone(),
            event_type: "submit",
            bubbles: true,
            cancelable: true,
        })?;
        merge_outcome(outcome, submit.outcome, self.page.dom.document.id());
        if !submit.default_allowed {
            return Ok(None);
        }
        if self.script_runtime.is_none()
            && form_node.attr("novalidate").is_none()
            && submitter
                .and_then(|id| self.page.dom.find_node(id))
                .is_none_or(|node| node.attr("formnovalidate").is_none())
            && let Some(first) = self.scriptless_invalid(&form_node).into_iter().next()
        {
            // Scriptless static validation blocks submission, reports the
            // first invalid control, and focuses it like interactive repair.
            first.update_control_state_tracked(|state| {
                state.reported = true;
            });
            self.focused_node = Some(first.id());
            super::request_state_render(&first, outcome);
            return Ok(None);
        }
        Ok(self.form_navigation(form_id, submitter))
    }

    /// Owned invalid controls for scriptless gating (no listeners to notify).
    fn scriptless_invalid(&self, form: &NodeRef) -> Vec<NodeRef> {
        crate::engine::dom::Node::static_invalid_controls(
            form,
            &self.page.dom.document,
            &crate::engine::dom::node::control_validity::PatternSource::Live(
                &crate::engine::pattern_eval::test_pattern,
            ),
        )
    }

    fn form_navigation(
        &self,
        form_id: NodeId,
        submitter: Option<NodeId>,
    ) -> Option<(String, NavigationDisposition)> {
        let form = self.layout.forms.get(&form_id)?;
        if form.method != "get" {
            return None;
        }
        let mut fields = form.hidden_fields.clone();
        for item in &self.layout.items {
            let DisplayItem::Control(control) = item else {
                continue;
            };
            if control.form_id != Some(form_id) || control.name.is_empty() {
                continue;
            }
            if matches!(
                control.kind,
                ControlKind::Text
                    | ControlKind::TextArea
                    | ControlKind::Password
                    | ControlKind::Search
                    | ControlKind::Select
            ) || (control.kind == ControlKind::Submit && Some(control.node_id) == submitter)
            {
                fields.push((control.name.clone(), control.value.clone()));
            }
        }
        let query = fields
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    crate::navigation::encode_www_form_component(name),
                    crate::navigation::encode_www_form_component(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() {
            form.action.clone()
        } else {
            let separator = if form.action.contains('?') { '&' } else { '?' };
            format!("{}{separator}{query}", form.action)
        };
        Some((url, NavigationDisposition::CurrentTab))
    }
}

fn navigation_disposition(input: PointerInput) -> NavigationDisposition {
    if input.modifiers.control || input.button == PointerButton::Middle {
        NavigationDisposition::NewBackgroundTab
    } else {
        NavigationDisposition::CurrentTab
    }
}
