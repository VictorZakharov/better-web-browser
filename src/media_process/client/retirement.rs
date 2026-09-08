//! Deferred source retirement shares the client's decode/command serialization boundary.
use super::*;

impl MediaClient {
    pub(crate) fn pause_retired_source(&mut self, source_id: u64) -> Result<bool, String> {
        // A replacement decode may win the client lock before a queued retirement. Sending
        // its predecessor's identity would violate the worker's strict source contract and
        // terminate that worker, including the replacement. Compare the installed identity
        // under the same exclusive client borrow used for decoding and playback commands.
        if self.active_source != Some(source_id) {
            return Ok(false);
        }
        self.set_playback(source_id, false, 0)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
