//! Deferred page-resource loading and UI-thread installation.

use super::*;

pub(super) fn load_page_resources(
    client: &winhttp::HttpClient,
    page: &mut Page,
    loaded: &mut HashSet<PageResource>,
    resource_budget: &mut u64,
    bytes: &mut u64,
    network_time: &mut Duration,
    resource_processing_time: &mut Duration,
) {
    const MAX_PARALLEL_FETCHES: usize = 24;
    let resources = page
        .resources
        .iter()
        .filter(|resource| {
            !loaded.contains(*resource) && page.resource_blocks_first_paint(resource)
        })
        .cloned()
        .collect::<Vec<_>>();

    for batch in resources.chunks(MAX_PARALLEL_FETCHES) {
        if *resource_budget == 0 {
            break;
        }
        for resource in batch {
            loaded.insert(resource.clone());
        }

        let batch_started = Instant::now();
        let responses = std::thread::scope(|scope| {
            let requests = batch
                .iter()
                .map(|resource| scope.spawn(move || client.get(page_resource_url(resource))))
                .collect::<Vec<_>>();
            requests
                .into_iter()
                .map(|request| {
                    request.join().unwrap_or_else(|_| {
                        Err("resource request worker terminated unexpectedly".into())
                    })
                })
                .collect::<Vec<_>>()
        });
        *network_time += batch_started.elapsed();

        let processing_started = Instant::now();
        for (resource, response) in batch.iter().cloned().zip(responses) {
            let Ok(response) = response else {
                continue;
            };
            if !response.is_success() {
                continue;
            }
            let size = response.body.len() as u64;
            if size > *resource_budget {
                continue;
            }

            let retained = match resource {
                PageResource::Stylesheet { url } => {
                    page.add_stylesheet_from(
                        &url,
                        winhttp::decode_text(&response.body, response.content_type.as_deref()),
                    );
                    true
                }
                PageResource::Image { url } => page.add_image(url, &response.body).is_ok(),
                PageResource::Script { url } => {
                    page.add_script(
                        &url,
                        winhttp::decode_text(&response.body, response.content_type.as_deref()),
                    );
                    true
                }
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => page
                    .add_font(url, family, weight, italic, &response.body)
                    .is_ok(),
            };
            if retained {
                *bytes += size;
                *resource_budget -= size;
                if *resource_budget == 0 {
                    break;
                }
            }
        }
        *resource_processing_time += processing_started.elapsed();
        if *resource_budget == 0 {
            break;
        }
    }
}

fn page_resource_url(resource: &PageResource) -> &str {
    match resource {
        PageResource::Stylesheet { url }
        | PageResource::Image { url }
        | PageResource::Script { url }
        | PageResource::Font { url, .. } => url,
    }
}

pub(super) struct DeferredResourcesMessage {
    pub generation: u64,
    pub loaded: Vec<(PageResource, Vec<u8>)>,
}

impl BrowserState {
    pub(super) fn unloaded_font_resources(&self) -> Vec<PageResource> {
        self.page
            .resources
            .iter()
            .filter(|resource| {
                matches!(resource, PageResource::Font { .. })
                    && !self.loaded_page_resources.contains(*resource)
            })
            .cloned()
            .collect()
    }

    pub(super) unsafe fn begin_deferred_resources(&self, resources: Vec<PageResource>) {
        if resources.is_empty() {
            return;
        }
        let generation = self.generation;
        let window = self.window as isize;
        let http_client = Arc::clone(&self.http_client);
        std::thread::spawn(move || {
            let client = http_client;
            let loaded = std::thread::scope(|scope| {
                let client = &client;
                let requests = resources
                    .into_iter()
                    .map(|resource| {
                        scope.spawn(move || {
                            let response = client.get(page_resource_url(&resource));
                            (resource, response)
                        })
                    })
                    .collect::<Vec<_>>();
                requests
                    .into_iter()
                    .filter_map(|request| request.join().ok())
                    .filter_map(|(resource, response)| {
                        response
                            .ok()
                            .filter(winhttp::HttpResponse::is_success)
                            .map(|response| (resource, response.body))
                    })
                    .collect::<Vec<_>>()
            });
            let message = Box::new(DeferredResourcesMessage { generation, loaded });
            let pointer = Box::into_raw(message);
            if unsafe {
                PostMessageW(
                    window as Hwnd,
                    WM_APP_DEFERRED_RESOURCES,
                    0,
                    pointer as isize,
                )
            } == 0
            {
                unsafe { drop(Box::from_raw(pointer)) };
            }
        });
    }

    pub(super) unsafe fn finish_deferred_resources(&mut self, message: DeferredResourcesMessage) {
        if message.generation != self.generation {
            return;
        }
        let mut changed = false;
        for (resource, body) in message.loaded {
            let size = body.len() as u64;
            if size > self.page_resource_budget {
                continue;
            }
            let installed = match &resource {
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => self
                    .page
                    .add_font(url.clone(), family.clone(), *weight, *italic, &body)
                    .is_ok(),
                _ => false,
            };
            if installed {
                self.page_resource_budget -= size;
                self.loaded_page_resources.insert(resource);
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.bytes += size;
                }
                changed = true;
            }
        }
        if changed {
            self.web_fonts.clear();
            self.web_fonts.register(&self.page.fonts);
            self.dynamic_fonts.clear();
            self.rebuild_layout();
            InvalidateRect(self.window, null(), 0);
        }
    }
}
