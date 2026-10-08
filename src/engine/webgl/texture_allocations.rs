//! Transactional per-image lifetime high-water reservations for texture storage.
//! Reusing a mip must not consume its complete allocation on every definition.
use super::{Kind, Result, WebGl, gl};

pub(super) struct Reservation {
    id: u32,
    previous_counter: usize,
    counter: usize,
    capacity: usize,
    images: Vec<((u32, i32), usize)>,
}
impl Reservation {
    pub(super) fn counter(&self) -> usize {
        self.counter
    }
}
impl WebGl {
    pub(super) fn prepare_texture_storage(
        &self,
        id: u32,
        images: Vec<((u32, i32), usize)>,
    ) -> Result<Reservation> {
        let object = self.objects.get(id, Kind::Texture)?;
        let mut seen = std::collections::HashSet::new();
        let mut growth = 0usize;
        if images.len() > 78 {
            return Err(gl::OUT_OF_MEMORY);
        }
        for &(key, bytes) in &images {
            if !seen.insert(key) {
                return Err(gl::INVALID_OPERATION);
            }
            let previous = object.texture_allocations.get(&key).copied().unwrap_or(0);
            growth = growth
                .checked_add(bytes.saturating_sub(previous))
                .ok_or(gl::OUT_OF_MEMORY)?;
        }
        let counter = self.admit_storage_growth(growth.checked_mul(2).ok_or(gl::OUT_OF_MEMORY)?)?;
        let capacity = object
            .capacity
            .checked_add(growth)
            .ok_or(gl::OUT_OF_MEMORY)?;
        Ok(Reservation {
            id,
            previous_counter: self.resource_bytes,
            counter,
            capacity,
            images,
        })
    }
    pub(super) fn commit_texture_storage(&mut self, reservation: Reservation) -> Result<()> {
        // Driver calls are synchronous and do not invoke author JavaScript.
        // A reservation must not span another browser allocation or callback.
        debug_assert_eq!(self.resource_bytes, reservation.previous_counter);
        let object = self.objects.get_mut(reservation.id, Kind::Texture)?;
        for (key, bytes) in reservation.images {
            object
                .texture_allocations
                .entry(key)
                .and_modify(|previous| *previous = (*previous).max(bytes))
                .or_insert(bytes);
        }
        object.capacity = reservation.capacity;
        self.resource_bytes = reservation.counter;
        Ok(())
    }
}
