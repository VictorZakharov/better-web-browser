//! Deferred page-resource loading and UI-thread installation.

use super::*;
use better_web_browser::fetch::{FetchRequest, FetchSignal, RequestDestination};

const MAX_PARALLEL_DEFERRED_FETCHES: usize = 4;

pub(super) struct ResourceLoadContext<'a> {
    pub client: &'a winhttp::HttpClient,
    pub signal: &'a FetchSignal,
    pub loaded: &'a mut HashSet<PageResource>,
    pub resource_budget: &'a mut u64,
    pub bytes: &'a mut u64,
    pub network_time: &'a mut Duration,
    pub processing_time: &'a mut Duration,
}

pub(super) fn load_page_resources(page: &mut Page, context: ResourceLoadContext<'_>) {
    let ResourceLoadContext {
        client,
        signal,
        loaded,
        resource_budget,
        bytes,
        network_time,
        processing_time: resource_processing_time,
    } = context;
    const MAX_PARALLEL_FETCHES: usize = 24;
    let resources = page
        .resources
        .iter()
        .filter(|resource| {
            !loaded.contains(*resource) && page.resource_blocks_first_paint(resource)
        })
        .cloned()
        .collect::<Vec<_>>();
    let document_url = page.source_url.clone();

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
                .map(|resource| {
                    let signal = signal.clone();
                    let document_url = &document_url;
                    scope.spawn(move || {
                        fetch_document_resource(
                            client,
                            &signal,
                            document_url,
                            page_resource_url(resource),
                            page_resource_destination(resource),
                        )
                    })
                })
                .collect::<Vec<_>>();
            requests
                .into_iter()
                .map(|request| {
                    request.join().unwrap_or_else(|_| {
                        Err(better_web_browser::fetch::FetchError::new(
                            better_web_browser::fetch::FetchErrorKind::Network,
                            "resource request worker terminated unexpectedly",
                        ))
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
                        winhttp::decode_text(response.body.as_bytes(), response.content_type()),
                    );
                    true
                }
                PageResource::Image { url } => {
                    page.add_image(url, response.body.as_bytes()).is_ok()
                }
                PageResource::Script { url } => {
                    page.add_script(
                        &url,
                        winhttp::decode_text(response.body.as_bytes(), response.content_type()),
                    );
                    true
                }
                PageResource::Font {
                    url,
                    family,
                    weight,
                    italic,
                } => page
                    .add_font(url, family, weight, italic, response.body.as_bytes())
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

fn page_resource_destination(resource: &PageResource) -> RequestDestination {
    match resource {
        PageResource::Stylesheet { .. } => RequestDestination::Style,
        PageResource::Image { .. } => RequestDestination::Image,
        PageResource::Script { .. } => RequestDestination::Script,
        PageResource::Font { .. } => RequestDestination::Font,
    }
}

pub(super) fn fetch_document_resource(
    client: &winhttp::HttpClient,
    signal: &FetchSignal,
    document_url: &str,
    url: &str,
    destination: RequestDestination,
) -> Result<winhttp::HttpResponse, better_web_browser::fetch::FetchError> {
    let request =
        FetchRequest::subresource(url, document_url, destination)?.with_signal(signal.clone());
    client.fetch(request)
}

pub(super) struct DeferredResourcesMessage {
    pub generation: u64,
    pub loaded: Vec<(PageResource, Vec<u8>)>,
}

impl BrowserState {
    pub(super) fn unloaded_deferred_resources(&self) -> Vec<PageResource> {
        self.page
            .resources
            .iter()
            .filter(|resource| {
                let deferred = match resource {
                    PageResource::Image { url } => !self.page.images.contains_key(url),
                    PageResource::Font { .. } => true,
                    _ => false,
                };
                deferred && !self.loaded_page_resources.contains(*resource)
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
        let fetch_signal = self.document_fetch.signal();
        let document_url = self.page.source_url.clone();
        let resource_budget = self.page_resource_budget;
        std::thread::spawn(move || {
            let client = http_client;
            let mut loaded = Vec::new();
            let mut remaining_budget = resource_budget;
            for batch in resources.chunks(MAX_PARALLEL_DEFERRED_FETCHES) {
                let fetched = std::thread::scope(|scope| {
                    let requests = batch
                        .iter()
                        .cloned()
                        .map(|resource| {
                            let client = &client;
                            let signal = fetch_signal.clone();
                            let document_url = &document_url;
                            scope.spawn(move || {
                                let response = fetch_document_resource(
                                    client,
                                    &signal,
                                    document_url,
                                    page_resource_url(&resource),
                                    page_resource_destination(&resource),
                                );
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
                                .map(|response| (resource, response.body.into_bytes()))
                        })
                        .collect::<Vec<_>>()
                });
                for (resource, body) in fetched {
                    let size = body.len() as u64;
                    if size <= remaining_budget {
                        remaining_budget -= size;
                        loaded.push((resource, body));
                    }
                }
                if remaining_budget == 0 {
                    break;
                }
            }
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
        let mut fonts_changed = false;
        for (resource, body) in message.loaded {
            let size = body.len() as u64;
            if size > self.page_resource_budget {
                continue;
            }
            let installed = match &resource {
                PageResource::Image { url } => self.page.add_image(url.clone(), &body).is_ok(),
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
                let is_font = matches!(resource, PageResource::Font { .. });
                self.page_resource_budget -= size;
                self.loaded_page_resources.insert(resource);
                if let Some(benchmark) = self.benchmark.as_mut() {
                    benchmark.bytes += size;
                }
                changed = true;
                fonts_changed |= is_font;
            }
        }
        if changed {
            if fonts_changed {
                self.web_fonts.clear();
                self.web_fonts.register(&self.page.fonts);
                self.dynamic_fonts.clear();
            }
            self.rebuild_layout();
            InvalidateRect(self.window, null(), 0);
        }
    }
}
