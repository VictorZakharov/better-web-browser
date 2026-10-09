//! A binding proof lives only within one synchronous native-owner batch.
use super::*;
use crate::engine::webgl::command_batch::Entry;
use std::collections::HashSet;

impl BackendContexts {
    pub(in crate::engine::webgl) fn execute_batch(
        &mut self,
        commands: Vec<(u32, Entry)>,
    ) -> Vec<u32> {
        let mut lost = HashSet::new();
        let mut current = None;
        for (id, entry) in commands {
            if lost.contains(&id) {
                continue;
            }
            let value = match entry {
                Entry::Numeric(command) => {
                    // NumericCommand admits only converted, reply-free setters.
                    // Their dispatcher does not create, replace or destroy EGL
                    // contexts. No peer runs on this thread during the loop.
                    let result = self.execute_command_bound(
                        id,
                        command.into_command(),
                        None,
                        current == Some(id),
                    );
                    current = Some(id);
                    result
                }
                Entry::Json(source) => {
                    // Do not infer a binding guarantee from a serialized string.
                    // The ordinary parser/dispatcher keeps its full admission
                    // and activation path, even for a canonical setter.
                    current = None;
                    self.execute(id, &source, None)
                }
            };
            if value.get("lost") == Some(&json!(true)) {
                lost.insert(id);
                // Poisoned-object retirement may activate then unbind a context.
                current = None;
            }
        }
        let mut lost: Vec<_> = lost.into_iter().collect();
        lost.sort_unstable();
        lost
    }
}

#[cfg(test)]
mod tests;
