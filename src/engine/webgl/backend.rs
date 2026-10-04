//! Native owner registry, aggregate budgets and bounded command dispatch.
use super::*;
/// The enclosing realm owns this registry and drops it on navigation/worker shutdown.
#[derive(Default)]
pub(super) struct BackendContexts {
    pub(super) contexts: HashMap<u32, WebGl>,
    next: u32,
}

impl BackendContexts {
    pub(crate) fn create(&mut self, width: u32, height: u32, options: &str) -> Option<u32> {
        if self.contexts.len() >= MAX_NATIVE_CONTEXTS {
            return None;
        }
        let options: Options = serde_json::from_str(options).ok()?;
        let surface_bytes = (width as usize).checked_mul(height as usize)?.checked_mul(
            if options.depth || options.stencil {
                8
            } else {
                4
            },
        )?;
        let existing: usize = self
            .contexts
            .values()
            .map(|context| context.resource_bytes)
            .sum();
        if existing.checked_add(surface_bytes)? > MAX_PROCESS_RESOURCE_BYTES {
            return None;
        }
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
            MAX_RESOURCE_BYTES.min(MAX_PROCESS_RESOURCE_BYTES.saturating_sub(other_bytes));
        if command.len() > MAX_SHADER_BYTES + 4096
            || bytes.is_some_and(|b| b.len() > MAX_UPLOAD_BYTES)
        {
            context.error(gl::OUT_OF_MEMORY);
            return Value::Null;
        }
        let command = match serde_json::from_str::<Command>(command) {
            Ok(command) => command,
            Err(_) => {
                context.error(gl::INVALID_VALUE);
                return Value::Null;
            }
        };
        if context.native.make_current().is_err() {
            return json!({"lost":true});
        }
        let result = context.dispatch(&command, bytes);
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
        let context = self.contexts.get_mut(&id)?;
        if context.objects.poisoned {
            return None;
        }
        context.native.make_current().ok()?;
        let mut pixels = context.surface.snapshot().ok()?;
        if !context.options.alpha {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        Some((context.surface.width, context.surface.height, pixels))
    }
}
