//! Storage admission is transactional; retirement waits for the last native
//! container reference. High-water reservations remain conservative while live.
use super::{Kind, Result, WebGl, gl};

pub(super) struct Reservation {
    id: u32,
    kind: Kind,
    previous_counter: usize,
    counter: usize,
    capacity: usize,
}

impl WebGl {
    pub(super) fn prepare_object_storage(
        &self,
        id: u32,
        kind: Kind,
        bytes: usize,
    ) -> Result<Reservation> {
        let previous = self.objects.get(id, kind)?.capacity;
        Ok(Reservation {
            id,
            kind,
            previous_counter: self.resource_bytes,
            counter: self.storage_counter(kind, previous, bytes)?,
            capacity: previous.max(bytes),
        })
    }

    pub(super) fn commit_object_storage(&mut self, reservation: Reservation) -> Result<()> {
        // No author callbacks or other allocations may span a reservation.
        debug_assert_eq!(self.resource_bytes, reservation.previous_counter);
        self.objects
            .get_mut(reservation.id, reservation.kind)?
            .capacity = reservation.capacity;
        self.resource_bytes = reservation.counter;
        Ok(())
    }

    fn storage_counter(&self, kind: Kind, previous: usize, next: usize) -> Result<usize> {
        // Two copies conservatively cover GPU storage and indexed-buffer CPU
        // mirrors. Live object capacities do not shrink on redefinition.
        let growth = next
            .saturating_sub(previous)
            .checked_mul(2)
            .ok_or(gl::OUT_OF_MEMORY)?;
        self.admit_storage_growth_for(Some(kind), growth)
    }

    pub(super) fn reclaim_retired_storage(&mut self) -> Result<()> {
        let capacity = self.objects.take_retired_capacity();
        let retired = capacity.checked_mul(2).ok_or(gl::OUT_OF_MEMORY)?;
        self.resource_bytes = self.resource_bytes.checked_sub(retired).ok_or_else(|| {
            // An accounting invariant violation cannot safely grant more memory.
            self.objects.poisoned = true;
            gl::OUT_OF_MEMORY
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
