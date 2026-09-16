//! Fetch-completion-driven preparation shared by import() and inserted module elements.
use super::*;
use crate::engine::script::dynamic_modules::{Job, Owner};

impl ScriptRuntime {
    pub(crate) fn take_module_requests(&mut self) -> Vec<(String, ScriptFetchOptions)> {
        if !self.is_active() || !std::mem::take(&mut self.host.borrow_mut().module_jobs.dirty) {
            return Vec::new();
        }
        let jobs = std::mem::take(&mut self.host.borrow_mut().module_jobs.pending);
        let mut requests = Vec::new();
        for mut job in jobs {
            match self.prepare_dynamic_module(&mut job, &mut requests) {
                Ok(false) => self.host.borrow_mut().module_jobs.pending.push(job),
                result => {
                    let result = result.map(|_| ());
                    if let Owner::Element(node) = &job.owner {
                        // A parse/link error is an executable errored module record, not a
                        // resource failure: report it globally and still fire element load.
                        let result = if job.script_error { Ok(()) } else { result };
                        self.host.borrow_mut().pending_dynamic_scripts.complete(
                            node.id(),
                            result.map(|_| job.source.unwrap_or_default()),
                            0,
                        );
                    } else {
                        self.host
                            .borrow_mut()
                            .module_jobs
                            .ready
                            .push_back((job, result));
                    }
                }
            }
        }
        // HTML removes failed fetch/MIME entries after notifying their current waiters.
        // Later import() calls can retry; compiled parse/evaluation errors stay cached.
        let mut host = self.host.borrow_mut();
        let failed = host
            .module_jobs
            .responses
            .iter()
            .filter(|(_, r)| r.is_err())
            .map(|(u, _)| u.clone())
            .collect::<Vec<_>>();
        for url in failed {
            host.module_jobs.requested.remove(&url);
            host.module_jobs.responses.remove(&url);
        }
        requests
    }

    fn prepare_dynamic_module(
        &mut self,
        job: &mut Job,
        requests: &mut Vec<(String, ScriptFetchOptions)>,
    ) -> Result<bool, String> {
        if job.source.is_none() {
            job.source = self.module_source(&job.url, job.options, requests)?;
        }
        let Some(source) = job.source.as_ref() else {
            return Ok(false);
        };
        self.context
            .as_mut()
            .unwrap()
            .register_script_origin(&job.url, &job.base, job.options);
        for _ in 0..MAX_DYNAMIC_SCRIPTS {
            let missing = self
                .prepare_module_graph(&job.url, source)
                .inspect_err(|_| job.script_error = true)?;
            if missing.is_empty() {
                return Ok(true);
            }
            let mut waiting = false;
            for url in missing {
                match self.module_source(&url, job.options, requests)? {
                    Some(source) => {
                        self.context.as_mut().unwrap().register_script_origin(
                            &url,
                            &url,
                            job.options,
                        );
                        self.install_module_dependency(&url, &source)?;
                    }
                    None => waiting = true,
                }
            }
            if waiting {
                return Ok(false);
            }
        }
        Err("module graph exceeds the dependency limit".into())
    }

    fn module_source(
        &mut self,
        url: &str,
        options: ScriptFetchOptions,
        requests: &mut Vec<(String, ScriptFetchOptions)>,
    ) -> Result<Option<String>, String> {
        let mut host = self.host.borrow_mut();
        if let Some(source) = host.module_loader.source(url) {
            return Ok(Some(source));
        }
        if let Some(Err(error)) = host.module_jobs.responses.get(url) {
            return Err(error.clone());
        }
        if !host.module_jobs.requested.contains_key(url) {
            if host.module_jobs.requested.len() >= MAX_DYNAMIC_SCRIPTS {
                return Err("module graph exceeds the dependency-count limit".into());
            }
            host.module_jobs.requested.insert(url.to_owned(), options);
            requests.push((url.to_owned(), options));
        }
        Ok(None)
    }

    pub(crate) fn complete_module_fetch(
        &mut self,
        url: String,
        result: Result<(String, String), String>,
    ) {
        if !self.is_active() {
            return;
        }
        // Charge bytes once on admission. The source cache is the only retained source owner.
        let result = result.and_then(|(base, source)| {
            self.install_module_dependency(&url, &source)?;
            let options = self
                .host
                .borrow()
                .module_jobs
                .requested
                .get(&url)
                .copied()
                .unwrap_or_else(|| ScriptFetchOptions::for_kind(ScriptKind::Module));
            let context = self.context.as_mut().unwrap();
            context.register_script_origin(&url, &base, options);
            context.set_module_response_base(&url, &base);
            Ok(())
        });
        let mut host = self.host.borrow_mut();
        host.module_jobs.responses.insert(url, result);
        host.module_jobs.dirty = true;
    }
}
