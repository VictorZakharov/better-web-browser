//! Document-owned asynchronous module graph jobs. Preparation never runs author code.
use super::*;
use std::collections::VecDeque;

#[derive(Clone)]
pub(super) enum Owner {
    Element(NodeRef),
    Import(u32),
}

pub(super) struct Job {
    pub owner: Owner,
    pub url: String,
    pub base: String,
    pub source: Option<String>,
    pub options: ScriptFetchOptions,
    pub script_error: bool,
}

#[derive(Default)]
pub(super) struct ModuleJobs {
    pub pending: Vec<Job>,
    pub ready: VecDeque<(Job, Result<(), String>)>,
    pub dirty: bool,
    pub requested: HashMap<String, ScriptFetchOptions>,
    pub responses: HashMap<String, Result<(), String>>,
    next_import: u32,
}

impl ModuleJobs {
    pub fn import(&mut self, url: String, options: ScriptFetchOptions) -> Result<u32, String> {
        if self.pending.len() + self.ready.len() >= MAX_DYNAMIC_SCRIPTS {
            return Err("too many pending module imports".into());
        }
        let id = self
            .next_import
            .checked_add(1)
            .ok_or("module import IDs exhausted")?;
        self.next_import = id;
        self.pending.push(Job {
            owner: Owner::Import(id),
            base: url.clone(),
            url,
            source: None,
            options,
            script_error: false,
        });
        self.dirty = true;
        Ok(id)
    }
}

pub(super) fn run_one_import(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
) -> bool {
    let Some((job, result)) = host.borrow_mut().module_jobs.ready.pop_front() else {
        return false;
    };
    let Owner::Import(id) = job.owner else {
        unreachable!("elements use the script queue")
    };
    host.borrow_mut().begin_task();
    if let Err(error) = context.finish_import(id, &job.url, result, job.script_error) {
        outcome
            .errors
            .push(format!("module import completion: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome.errors.push(error.to_string());
    }
    super::module_lifecycle::drain(context, host, outcome);
    true
}
