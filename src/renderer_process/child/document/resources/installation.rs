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
        integrity_by_request: &mut HashMap<u64, Vec<String>>,
        require_authoritative_match: bool,
    ) -> Result<bool, String> {
        let mut retained = false;
        for response in responses {
            let Some(resource) = by_request.remove(&response.head.request_id) else {
                return Err("browser returned an unknown resource request".into());
            };
            let integrity = integrity_by_request
                .remove(&response.head.request_id)
                .unwrap_or_default();
            let label = resource_label(&resource);
            // A speculative source may finish before its element has been parsed. Retain the
            // validated, budgeted bytes; only enqueueing the actual element permits execution.
            let admitted = self.page.resources.contains(&resource)
                || self.parser_scripts.contains(&resource)
                || (self.parser.is_some() && matches!(resource, PageResource::Script { .. }));
            let current_media = !matches!(resource, PageResource::Media { .. })
                || self.page.is_current_media_resource(&resource);
            if !current_media
                || !self.page.is_current_font_resource(&resource)
                || (require_authoritative_match && !admitted)
            {
                continue;
            }
            self.loaded_resources.insert(resource.clone());
            let response = match into_fetch_result(response) {
                Ok(response) => response,
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    if self.page.retry_font_resource(&resource) {
                        continue;
                    }
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                    continue;
                }
            };
            if matches!(resource, PageResource::Prefetch { .. }) {
                // HTML prefetch fires load for any completed HTTP response, including 404.
                // Only a network error (handled above) fires error.
                retained |= self.dispatch_resource_event(&resource, "load")?;
                continue;
            }
            if !response.is_success() {
                self.record_resource_diagnostic(format!(
                    "{label}: server returned HTTP {}",
                    response.status
                ));
                if !self.page.retry_font_resource(&resource) {
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                }
                continue;
            }
            if matches!(resource, PageResource::OriginHint { .. }) {
                // DNS has no response body and never mutates the active document.
                continue;
            }
            let eligible = matches!(
                response.response_type,
                crate::fetch::ResponseType::Basic | crate::fetch::ResponseType::Cors
            );
            if matches!(resource, PageResource::Font { .. }) && !eligible {
                self.record_resource_diagnostic(format!(
                    "{label}: font response is not origin-clean"
                ));
                if !self.page.retry_font_resource(&resource) {
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                }
                continue;
            }
            if let Some(error) = integrity.iter().find_map(|metadata| {
                crate::fetch::integrity::verify(metadata, response.body.as_bytes(), eligible).err()
            }) {
                self.record_resource_diagnostic(format!("{label}: {error}"));
                retained |= self.dispatch_resource_event(&resource, "error")?;
                continue;
            }
            if matches!(resource, PageResource::Preload { .. }) {
                if matches!(
                    resource,
                    PageResource::Preload {
                        as_type: crate::engine::page::PreloadAs::ModuleScript,
                        ..
                    }
                ) && let Err(error) = validate_script_response(&response, ScriptKind::Module)
                {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                    continue;
                }
                if self.store_preload(&resource, response) {
                    retained |= self.dispatch_resource_event(&resource, "load")?;
                } else {
                    self.record_resource_diagnostic(format!(
                        "{label}: document preload cache limit reached"
                    ));
                    retained |= self.dispatch_resource_event(&resource, "error")?;
                }
                continue;
            }
            if matches!(resource, PageResource::Stylesheet { .. })
                && !super::stylesheets::valid_response(&self.page, &response)
            {
                self.record_resource_diagnostic(format!(
                    "{label}: response is not a CSS stylesheet (Content-Type mismatch)"
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
            let final_url = response.final_url().as_str().to_string();
            let bytes = response.body.into_bytes();
            let installed = match resource {
                PageResource::Preload { .. } => unreachable!("preload handled before installation"),
                PageResource::OriginHint { .. } => {
                    unreachable!("network hint handled before installation")
                }
                PageResource::Prefetch { .. } => {
                    unreachable!("prefetch handled before installation")
                }
                PageResource::Stylesheet { url } => self
                    .page
                    .add_linked_stylesheet_response(
                        &url,
                        &final_url,
                        crate::winhttp::decode_text(&bytes, content_type.as_deref()),
                    )
                    .then_some(())
                    .ok_or_else(|| "stylesheet was not installed".to_string()),
                PageResource::Image { url } => self.page.add_image(url, &bytes),
                PageResource::Media { node, kind, .. } => {
                    let mime_type = content_type.unwrap_or_else(|| match kind {
                        crate::engine::page::MediaElementKind::Audio => "audio/mpeg".into(),
                        crate::engine::page::MediaElementKind::Video => "video/mp4".into(),
                    });
                    let result = connection.decode_media(&bytes).and_then(|decode| {
                        self.install_media_decode(node, decode, mime_type, eligible)
                    });
                    if let Err(error) = &result {
                        self.record_media_failure(error.clone());
                    }
                    result
                }
                PageResource::Script {
                    url,
                    kind,
                    fetch_options,
                    ..
                } => {
                    let code = crate::winhttp::decode_text(&bytes, content_type.as_deref());
                    if code.len() > crate::limits::MAX_SCRIPT_BYTES {
                        self.record_resource_diagnostic(format!(
                            "{label}: script exceeds the per-script byte limit"
                        ));
                        retained |= self.dispatch_resource_event(&event_resource, "error")?;
                        continue;
                    }
                    let prepared =
                        self.parser.is_some() || self.parser_scripts.contains(&event_resource);
                    self.parser_scripts.complete(&event_resource, Some(&code));
                    (self.page.add_script(&url, kind, fetch_options, code) || prepared)
                        .then_some(())
                        .ok_or_else(|| "script was not installed".to_string())
                }
                PageResource::Font {
                    url: _,
                    source_url,
                    fallback_urls: _,
                    family,
                    weight,
                    italic,
                    unicode_range,
                    font_feature_settings,
                } => self.page.add_font_face(
                    crate::engine::font::WebFontFace {
                        url: source_url,
                        fallback_urls: Vec::new(),
                        family,
                        weight,
                        weight_min: f32::from(weight),
                        weight_max: f32::from(weight),
                        italic,
                        unicode_range,
                        features: crate::engine::css::FontFeatures::parse(&font_feature_settings)
                            .ok_or_else(|| "invalid font feature settings".to_owned())?,
                    },
                    &bytes,
                ),
            };
            match installed {
                Ok(()) => {
                    if let PageResource::Image { url } = &event_resource {
                        self.page.set_image_origin_clean(url, eligible);
                        if let Some(runtime) = self.script_runtime.as_mut() {
                            self.page.synchronize_script_images(runtime);
                        }
                    }
                    if matches!(event_resource, PageResource::Font { .. })
                        && let Some(runtime) = self.script_runtime.as_mut()
                    {
                        runtime.set_loaded_font_urls(&self.page.fonts);
                    }
                    // Fetching source alone does not change the rendered document. Its later
                    // script task and load handlers carry their own DOM invalidation.
                    retained |= !matches!(event_resource, PageResource::Script { .. });
                    self.resource_budget = self.resource_budget.saturating_sub(size);
                    retained |= self.dispatch_resource_event(&event_resource, "load")?;
                }
                Err(error) => {
                    self.record_resource_diagnostic(format!("{label}: {error}"));
                    if self.page.retry_font_resource(&event_resource) {
                        continue;
                    }
                    retained |= if matches!(event_resource, PageResource::Media { .. }) {
                        self.dispatch_media_failure(&event_resource, "decode")?
                    } else {
                        self.dispatch_resource_event(&event_resource, "error")?
                    };
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
