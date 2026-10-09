//! Native owner registry, aggregate budgets and bounded command dispatch.
use super::*;
mod batch;
/// The enclosing realm owns this registry and drops it on navigation/worker shutdown.
#[derive(Default)]
pub(super) struct BackendContexts {
    pub(super) contexts: HashMap<u32, WebGl>,
    next: u32,
    #[cfg(test)]
    activation_checks: u64,
}

impl BackendContexts {
    pub(super) fn set_profiling(&mut self, ids: &[u32], enabled: bool) {
        for id in ids.iter().take(MAX_CONTEXTS) {
            if let Some(context) = self.contexts.get_mut(id) {
                context
                    .execution_profile
                    .enable(enabled, context.resource_bytes);
            }
        }
    }
    pub(crate) fn create(&mut self, width: u32, height: u32, options: &str) -> Option<u32> {
        if self.contexts.len() >= MAX_NATIVE_CONTEXTS {
            return None;
        }
        let options: Options = serde_json::from_str(options).ok()?;
        let existing: usize = self
            .contexts
            .values()
            .map(|context| context.resource_bytes)
            .sum();
        let available = process_headroom::available(
            resource_ceiling().min(owner_resource_ceiling().saturating_sub(existing)),
            crate::process_memory::current().map(|sample| sample.private),
        );
        let (width, height) = drawing_buffer_extent::admitted(width, height, options, available)?;
        let next = self.next.checked_add(1)?;
        let context = WebGl::new(width, height, options).ok()?;
        self.next = next;
        self.contexts.insert(self.next, context);
        Some(self.next)
    }
    pub(crate) fn remove(&mut self, id: u32) {
        self.contexts.remove(&id);
    }
    pub(crate) fn execute(&mut self, id: u32, command: &str, bytes: Option<&[u8]>) -> Value {
        self.execute_data(id, command, bytes.map(std::borrow::Cow::Borrowed))
    }
    pub(crate) fn execute_owned(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<Vec<u8>>,
    ) -> Value {
        self.execute_data(id, command, bytes.map(std::borrow::Cow::Owned))
    }
    fn execute_data(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> Value {
        if command.len() > MAX_COMMAND_BYTES
            || bytes.as_ref().is_some_and(|b| b.len() > MAX_UPLOAD_BYTES)
        {
            return self.command_error(id, gl::OUT_OF_MEMORY);
        }
        let command = match serde_json::from_str::<Command>(command) {
            Ok(command) => command,
            Err(_) => return self.command_error(id, gl::INVALID_VALUE),
        };
        self.execute_command(id, command, bytes)
    }

    fn command_error(&mut self, id: u32, error: u32) -> Value {
        let Some(context) = self.contexts.get_mut(&id) else {
            return json!({"lost":true});
        };
        context.error(error);
        Value::Null
    }

    fn execute_command(
        &mut self,
        id: u32,
        command: Command,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> Value {
        self.execute_command_bound(id, command, bytes, false)
    }

    // Only the synchronous numeric batch coordinator may supply a prior-binding
    // proof. It never retains that proof across an owner mailbox operation.
    fn execute_command_bound(
        &mut self,
        id: u32,
        command: Command,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
        already_current: bool,
    ) -> Value {
        let other_bytes: usize = self
            .contexts
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, context)| context.resource_bytes)
            .sum();
        let Some(context) = self.contexts.get_mut(&id) else {
            return json!({"lost":true});
        };
        context.resource_limit =
            resource_ceiling().min(owner_resource_ceiling().saturating_sub(other_bytes));
        let sample = context.execution_profile.start();
        #[cfg(test)]
        if !already_current {
            self.activation_checks += 1;
        }
        if !already_current && context.native.make_current().is_err() {
            context.execution_profile.finish(
                execution_profile::Category::command(&command.op),
                sample,
                context.resource_bytes,
            );
            return json!({"lost":true});
        }
        let result = context.dispatch_data(&command, bytes);
        if matches!(command.op.as_str(), "compileShader" | "linkProgram")
            && let Err(error) = context.progress_program_links(1)
        {
            // Pipeline ready links while later shader pairs are submitted,
            // without waiting for unfinished translations or mailbox space.
            context.error(error);
        }
        context.execution_profile.finish(
            execution_profile::Category::command(&command.op),
            sample,
            context.resource_bytes,
        );
        if context.objects.poisoned {
            self.remove(id);
            return json!({"lost":true});
        }
        match result {
            Ok(value) => value,
            Err(error) => {
                context.error(error);
                Value::Null
            }
        }
    }
    pub(crate) fn snapshot(&mut self, id: u32) -> Option<(u32, u32, Vec<u8>)> {
        self.snapshot_pixels(id, false)
    }

    pub(super) fn canvas_snapshot(&mut self, id: u32) -> Option<(u32, u32, Vec<u8>)> {
        self.snapshot_pixels(id, true)
    }

    fn snapshot_pixels(&mut self, id: u32, straight_alpha: bool) -> Option<(u32, u32, Vec<u8>)> {
        let context = self.contexts.get_mut(&id)?;
        if context.objects.poisoned {
            return None;
        }
        let sample = context.execution_profile.start();
        let result = context
            .native
            .make_current()
            .and_then(|()| context.surface.snapshot());
        let result = result.map(|mut pixels| {
            if !context.options.alpha {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
            } else if straight_alpha && context.options.premultiplied_alpha {
                // Raw GL readPixels and native snapshots remain unchanged.
                // Only a Canvas bitmap consumer requests straight RGBA, once,
                // on the already-owned bytes before crossing the realm bridge.
                crate::engine::image_decode::unpremultiply_rgba(&mut pixels);
            }
            (context.surface.width, context.surface.height, pixels)
        });
        context.execution_profile.finish(
            execution_profile::Category::Readback,
            sample,
            context.resource_bytes,
        );
        result.ok()
    }
}
