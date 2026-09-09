//! Response validation, resource installation, and terminal completion events.
use super::*;
const MAX_RESOURCE_DIAGNOSTICS: usize = 32;
const MAX_RESOURCE_DIAGNOSTIC_BYTES: usize = 512;

impl DocumentRuntime {
    pub(super) fn install_resource_responses(
        &mut self,
        connection: &mut ChildConnection,
        responses: Vec<BrowserFetchResponse>,
        by_request: &mut HashMap<u64, PageResource>,
        require_authoritative_match: bool,
    ) -> Result<bool, String> {
        let mut retained = false;
        for response in responses {
            let Some(resource) = by_request.remove(&response.head.request_id) else {
                return Err("browser returned an unknown resource request".into());
            };
            let label = resource_label(&resource);
            if require_authoritative_match
                && !self.page.resources.contains(&resource)
                && !self.parser_scripts.contains(&resource)
            {
                continue;
            }
            self.loaded_resources.insert(resource.clone());
            let response = match into_fetch_result(response) {
                Ok(response) => response,
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                    continue;
                }
            };
            if !response.is_success() {
                self.record_resource_diagnostic(format!(
                    "{label}: server returned HTTP {}",
                    response.status
                ));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            if let PageResource::Script { kind, .. } = &resource
                && let Err(error) = validate_script_response(&response, *kind)
            {
                self.record_resource_diagnostic(format!("{label}: {error}"));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            let size = response.body.len() as u64;
            if size > self.resource_budget {
                self.record_resource_diagnostic(format!(
                    "{label}: skipped {size} bytes because only {} page-resource bytes remain",
                    self.resource_budget
                ));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            let event_resource = resource.clone();
            let content_type = response.content_type().map(str::to_string);
            let bytes = response.body.into_bytes();
            let installed = match resource {
                PageResource::Stylesheet { url } => self
                    .page
                    .add_stylesheet_from(
                        &url,
                        crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                    )
                    .then_some(())
                    .ok_or_else(|| "stylesheet was not installed".to_string()),
                PageResource::Image { url } => self.page.add_image(url, &bytes),
                PageResource::Media { node, .. } => {
                    let mime_type = content_type.unwrap_or_else(|| "video/mp4".into());
                    let result = connection
                        .decode_media(&bytes)
                        .and_then(|decode| self.install_media_decode(node, decode, mime_type));
                    if let Err(error) = &result {
                        self.record_media_failure(error.clone());
                    }
                    result
                }
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                } => {
                    let code = crate::winhttp::decode_text(&bytes, content_type.as_deref());
                    if code.len() > crate::limits::MAX_SCRIPT_BYTES {
                        self.record_resource_diagnostic(format!(
                            "{label}: script exceeds the per-script byte limit"
                        ));
                        retained |= self.dispatch_resource_event(&event_resource, "error")?;
                        continue;
                    }
                    let prepared = self.parser_scripts.contains(&event_resource);
                    self.parser_scripts.complete(&event_resource, Some(&code));
                    (self.page.add_script(&url, kind, fetch_options, code) || prepared)
                        .then_some(())
                        .ok_or_else(|| "script was not installed".to_string())
                }
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => self.page.add_font(url, family, weight, italic, &bytes),
            };
            match installed {
                Ok(()) => {
                    retained = true;
                    self.resource_budget = self.resource_budget.saturating_sub(size);
                    retained |= self.dispatch_resource_event(&event_resource, "load")?;
                }
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    retained |= self.dispatch_resource_event(&event_resource, "error")?;
                }
            }
        }
        Ok(retained)
    }

    fn record_resource_diagnostic(&mut self, message: String) {
        if self.page.diagnostics.len() >= MAX_RESOURCE_DIAGNOSTICS {
            return;
        }
        self.page.diagnostics.push(
            bounded_utf8_prefix(&message, MAX_RESOURCE_DIAGNOSTIC_BYTES)
                .0
                .to_string(),
        );
    }
}
