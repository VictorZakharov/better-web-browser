//! Fetch-completion-driven module graph preparation. Compiling never evaluates a module.
use super::*;
use crate::engine::dom::NodeId;
use crate::limits::MAX_DYNAMIC_SCRIPTS;

pub(super) struct ModuleGraphs {
    requested: HashSet<PageResource>,
    responses: HashMap<PageResource, Option<String>>,
    installed: HashSet<PageResource>,
    ready: HashMap<NodeId, Result<(), String>>,
    script_errors: HashSet<NodeId>,
    dirty: bool,
}

impl Default for ModuleGraphs {
    fn default() -> Self {
        Self {
            requested: HashSet::new(),
            responses: HashMap::new(),
            installed: HashSet::new(),
            ready: HashMap::new(),
            script_errors: HashSet::new(),
            dirty: true,
        }
    }
}

impl ModuleGraphs {
    pub(super) fn contains(&self, resource: &PageResource) -> bool {
        self.requested.contains(resource)
    }

    pub(super) fn resources(&self) -> impl Iterator<Item = PageResource> + '_ {
        self.requested
            .iter()
            .filter(|resource| !self.responses.contains_key(*resource))
            .cloned()
    }

    pub(super) fn complete(&mut self, resource: &PageResource, code: Option<&str>) {
        if matches!(
            resource,
            PageResource::Script {
                kind: ScriptKind::Module,
                ..
            }
        ) {
            self.responses
                .insert(resource.clone(), code.map(str::to_owned));
            self.dirty = true;
        }
    }

    pub(super) fn is_ready(&self, script: &PageScript) -> bool {
        self.ready.contains_key(&script.node.id())
    }

    pub(super) fn error(&self, script: &PageScript) -> Option<&str> {
        self.ready
            .get(&script.node.id())
            .and_then(|result| result.as_ref().err())
            .map(String::as_str)
    }

    pub(super) fn is_script_error(&self, script: &PageScript) -> bool {
        self.script_errors.contains(&script.node.id())
    }

    pub(super) fn prepare<'a>(
        &mut self,
        runtime: &mut ScriptRuntime,
        scripts: impl Iterator<Item = &'a PageScript>,
    ) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        for script in
            scripts.filter(|script| script.kind == ScriptKind::Module && script.code.is_some())
        {
            if self.is_ready(script) {
                continue;
            }
            match self.prepare_one(runtime, script) {
                Ok(false) => (),
                Ok(true) => {
                    self.ready.insert(script.node.id(), Ok(()));
                }
                Err(error) => {
                    self.ready.insert(script.node.id(), Err(error));
                }
            }
        }
    }

    fn prepare_one(
        &mut self,
        runtime: &mut ScriptRuntime,
        script: &PageScript,
    ) -> Result<bool, String> {
        for _ in 0..MAX_DYNAMIC_SCRIPTS {
            let missing = match runtime
                .prepare_module_graph(&script.source_url, script.code.as_deref().unwrap())
            {
                Ok(missing) => missing,
                Err(error) => {
                    self.script_errors.insert(script.node.id());
                    return Err(error);
                }
            };
            if missing.is_empty() {
                return Ok(true);
            }
            let mut installed = false;
            for url in missing {
                let resource = PageResource::Script {
                    url: url.clone(),
                    kind: ScriptKind::Module,
                    fetch_options: script.fetch_options,
                };
                if !self.requested.contains(&resource)
                    && self.requested.len() >= MAX_DYNAMIC_SCRIPTS
                {
                    return Err("module graph exceeds the dependency-count limit".into());
                }
                self.requested.insert(resource.clone());
                match self.responses.get(&resource) {
                    Some(Some(source)) if !self.installed.contains(&resource) => {
                        if let Err(error) = runtime.install_module_dependency(&url, source) {
                            self.responses.insert(resource, None);
                            return Err(error);
                        }
                        self.installed.insert(resource);
                        installed = true;
                    }
                    Some(None) => {
                        return Err(format!("module dependency could not be loaded: {url}"));
                    }
                    Some(Some(_)) | None => (),
                }
            }
            if !installed {
                return Ok(false);
            }
        }
        Err("module graph exceeds the preparation limit".into())
    }
}
