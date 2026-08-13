//! Deferred page-resource loading and UI-thread installation.

use super::*;

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
