//! A binding proof lives only within one synchronous native-owner batch.
use super::*;
use crate::command_batch::Entry;
use std::collections::HashSet;

impl BackendContexts {
    pub(crate) fn execute_batch(&mut self, commands: Vec<(u32, Entry)>) -> Vec<u32> {
        let mut lost = HashSet::new();
        let mut current = None;
        let mut converted = Command {
            op: String::with_capacity(64),
            i: Vec::with_capacity(8),
            f: Vec::with_capacity(16),
            text: String::new(),
        };
        for (id, entry) in commands {
            if lost.contains(&id) {
                continue;
            }
            let value = match entry {
                Entry::Numeric(command) => {
                    // NumericCommand admits only converted, reply-free setters.
                    // Their dispatcher does not create, replace or destroy EGL
                    // contexts. No peer runs on this thread during the loop.
                    command.write_command(&mut converted);
                    let result =
                        self.execute_command_bound(id, &converted, None, current == Some(id));
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
                Entry::Upload(source, bytes) => {
                    current = None;
                    self.execute_owned(id, &source, Some(bytes))
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
