//! Bounded native event ownership. Completion and success remain separate.
use std::collections::VecDeque;

pub(super) const SHADERS: usize = 64;
pub(super) const PROGRAMS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub id: u32,
    pub native: u32,
}

#[derive(Default)]
pub(super) struct Events {
    shaders: VecDeque<Event>,
    programs: VecDeque<Event>,
}

impl Events {
    pub fn len(&self, shader: bool) -> usize {
        self.queue(shader).len()
    }

    pub fn pop(&mut self, shader: bool) -> Option<Event> {
        self.queue_mut(shader).pop_front()
    }

    pub fn remove(&mut self, shader: bool, id: u32) {
        self.queue_mut(shader).retain(|event| event.id != id);
    }

    pub fn record(&mut self, shader: bool, event: Event) -> Result<(), Event> {
        let queue = self.queue_mut(shader);
        if queue.iter().any(|entry| entry.id == event.id)
            || queue.len() >= if shader { SHADERS } else { PROGRAMS }
        {
            return Err(event);
        }
        queue.push_back(event);
        Ok(())
    }

    fn queue(&self, shader: bool) -> &VecDeque<Event> {
        if shader {
            &self.shaders
        } else {
            &self.programs
        }
    }

    fn queue_mut(&mut self, shader: bool) -> &mut VecDeque<Event> {
        if shader {
            &mut self.shaders
        } else {
            &mut self.programs
        }
    }
}

#[cfg(test)]
mod tests;
