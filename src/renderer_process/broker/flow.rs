//! Consumption credit spans the network producer, pipe, and JavaScript stream.
//! Only network threads wait here; the broker continues handling input and cancellation.
use crate::limits::{MAX_FETCH_STREAM_IN_FLIGHT_BYTES, MAX_FETCH_STREAM_WINDOW_BYTES};
use crate::renderer_protocol::DocumentId;
use std::collections::HashMap;
use std::sync::{Condvar, Mutex};

#[derive(Default)]
pub(super) struct FetchFlow {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    requests: HashMap<(DocumentId, u64), Credit>,
    in_flight: usize,
}

struct Credit {
    limited: bool,
    reserved: u32,
    consumed: u32,
}

#[cfg(test)]
mod tests;

impl FetchFlow {
    pub(super) fn register(
        &self,
        document: DocumentId,
        id: u64,
        limited: bool,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.requests.contains_key(&(document, id)) {
            return Err("duplicate Fetch flow identity".into());
        }
        state.requests.insert(
            (document, id),
            Credit {
                limited,
                reserved: 0,
                consumed: 0,
            },
        );
        Ok(())
    }

    pub(super) fn reserve(
        &self,
        document: DocumentId,
        id: u64,
        offset: u32,
        bytes: usize,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        loop {
            let credit = state
                .requests
                .get(&(document, id))
                .ok_or_else(|| "Fetch response was cancelled or retired".to_string())?;
            if credit.reserved != offset {
                return Err("Fetch producer offset does not match reserved bytes".into());
            }
            if !credit.limited
                || (credit.reserved as usize - credit.consumed as usize + bytes
                    <= MAX_FETCH_STREAM_WINDOW_BYTES
                    && state.in_flight + bytes <= MAX_FETCH_STREAM_IN_FLIGHT_BYTES)
            {
                let credit = state.requests.get_mut(&(document, id)).unwrap();
                credit.reserved = credit
                    .reserved
                    .checked_add(bytes as u32)
                    .ok_or_else(|| "Fetch response credit overflow".to_string())?;
                if credit.limited {
                    state.in_flight += bytes;
                }
                return Ok(());
            }
            state = self.changed.wait(state).unwrap();
        }
    }

    pub(super) fn consume(&self, document: DocumentId, id: u64, total: u32) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        // A final receipt can race EOF, abort, or navigation. IDs are never reused.
        let Some(credit) = state.requests.get_mut(&(document, id)) else {
            return Ok(());
        };
        if total < credit.consumed || total > credit.reserved {
            return Err("Fetch consumption exceeds delivered bytes or moves backwards".into());
        }
        let freed = total - credit.consumed;
        credit.consumed = total;
        if credit.limited {
            state.in_flight -= freed as usize;
        }
        self.changed.notify_all();
        Ok(())
    }

    pub(super) fn has_capacity(
        &self,
        document: DocumentId,
        id: u64,
        bytes: usize,
    ) -> Result<bool, String> {
        let state = self.state.lock().unwrap();
        let credit = state
            .requests
            .get(&(document, id))
            .ok_or("Fetch response was cancelled or retired")?;
        Ok(!credit.limited
            || (credit.reserved as usize - credit.consumed as usize + bytes
                <= MAX_FETCH_STREAM_WINDOW_BYTES
                && state.in_flight + bytes <= MAX_FETCH_STREAM_IN_FLIGHT_BYTES))
    }

    pub(super) fn try_reserve(
        &self,
        document: DocumentId,
        id: u64,
        offset: u32,
        bytes: usize,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().unwrap();
        let credit = state
            .requests
            .get(&(document, id))
            .ok_or("Fetch response was cancelled or retired")?;
        if credit.reserved != offset {
            return Err("Fetch producer offset does not match reserved bytes".into());
        }
        if credit.limited
            && (credit.reserved as usize - credit.consumed as usize + bytes
                > MAX_FETCH_STREAM_WINDOW_BYTES
                || state.in_flight + bytes > MAX_FETCH_STREAM_IN_FLIGHT_BYTES)
        {
            return Ok(false);
        }
        let credit = state.requests.get_mut(&(document, id)).unwrap();
        credit.reserved = credit
            .reserved
            .checked_add(bytes as u32)
            .ok_or("Fetch response credit overflow")?;
        if credit.limited {
            state.in_flight += bytes;
        }
        Ok(true)
    }

    pub(super) fn retire(&self, document: DocumentId, id: u64) {
        let mut state = self.state.lock().unwrap();
        if let Some(credit) = state.requests.remove(&(document, id))
            && credit.limited
        {
            state.in_flight -= (credit.reserved - credit.consumed) as usize;
        }
        self.changed.notify_all();
    }

    pub(super) fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.requests.clear();
        state.in_flight = 0;
        self.changed.notify_all();
    }
}
