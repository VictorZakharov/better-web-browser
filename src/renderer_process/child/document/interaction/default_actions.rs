//! Renderer-owned link and form default actions after cancelable DOM dispatch.

use super::*;

impl DocumentRuntime {
    pub(super) fn pointer_default_action(
        &mut self,
        target: Option<&HitTarget>,
        input: PointerInput,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
        let Some(target) = target else {
            return Ok(None);
        };
        if let Some(url) = target.link.as_ref() {
            return Ok(Some((url.clone(), navigation_disposition(input))));
        }
        let Some(control) = target.control.as_ref() else {
            return Ok(None);
        };
        if control.kind == ControlKind::Reset {
            let Some(form_id) = control.form_id else {
                return Ok(None);
            };
            let Some(form_node) = self.page.dom.find_node(form_id) else {
                return Ok(None);
            };
            let reset = self.dispatch_user_input(
                UserInputEvent::Simple {
                    target: form_node,
                    event_type: "reset",
                    bubbles: true,
                    cancelable: true,
                },
                connection,
            )?;
            merge_outcome(outcome, reset.outcome, self.page.dom.document.id());
            return Ok(None);
        }
        if !matches!(control.kind, ControlKind::Submit) {
            return Ok(None);
        }
        let Some(form_id) = control.form_id else {
            return Ok(None);
        };
        self.submit_form(form_id, Some(control.node_id), connection, outcome)
    }

    pub(super) fn keyboard_default_action(
        &mut self,
        target: Option<NodeId>,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
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
        self.submit_form(form_id, submitter, connection, outcome)
    }

    fn submit_form(
        &mut self,
        form_id: NodeId,
        submitter: Option<NodeId>,
        connection: &mut ChildConnection,
        outcome: &mut ScriptOutcome,
    ) -> Result<Option<(String, NavigationDisposition)>, String> {
        let Some(form_node) = self.page.dom.find_node(form_id) else {
            return Ok(None);
        };
        let submit = self.dispatch_user_input(
            UserInputEvent::Simple {
                target: form_node,
                event_type: "submit",
                bubbles: true,
                cancelable: true,
            },
            connection,
        )?;
        merge_outcome(outcome, submit.outcome, self.page.dom.document.id());
        if !submit.default_allowed {
            return Ok(None);
        }
        Ok(self.form_navigation(form_id, submitter))
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
