//! Browser-instance storage authority and bounded, lossless recipient queues.
//! A full recipient applies backpressure before commit, never drops a broadcast.
use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Weak};

type Wake = Arc<dyn Fn() + Send + Sync>;

struct Endpoint {
    origin: String,
    queue: VecDeque<StorageUpdate>,
    bytes: usize,
    sequences: [u64; 2],
    wake: Option<Wake>,
    error: Option<String>,
}

pub struct StorageSubscription(Arc<Mutex<Endpoint>>);

impl StorageSubscription {
    pub fn take_error(&self) -> Option<String> {
        self.0.lock().expect("storage endpoint lock").error.take()
    }
    pub fn has_pending(&self) -> bool {
        !self
            .0
            .lock()
            .expect("storage endpoint lock")
            .queue
            .is_empty()
    }
    pub fn set_notifier(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.0.lock().expect("storage endpoint lock").wake = Some(Arc::new(wake));
    }

    /// The consumer must admit the front update before removing it. This keeps
    /// backpressure intact across the next IPC queue, without reordering on retry.
    pub fn take_if(&self, accepts: impl FnOnce(&StorageUpdate) -> bool) -> Option<StorageUpdate> {
        let mut endpoint = self.0.lock().expect("storage endpoint lock");
        if !endpoint.queue.front().is_some_and(accepts) {
            return None;
        }
        let update = endpoint.queue.pop_front()?;
        endpoint.bytes -= update.byte_len();
        Some(update)
    }
}

pub struct StorageCoordinator {
    local: Arc<LocalStorage>,
    endpoints: Mutex<Vec<Weak<Mutex<Endpoint>>>>,
}

impl StorageCoordinator {
    pub fn new(local: Arc<LocalStorage>) -> Self {
        Self {
            local,
            endpoints: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot and membership are established under the same authority lock.
    /// Dropping the subscription retires the document, including queued events.
    pub fn subscribe(
        &self,
        url: &str,
    ) -> Result<(StorageAreaSnapshot, StorageSubscription), StorageError> {
        let origin = storage_origin(url)?;
        let mut endpoints = self.endpoints.lock().expect("storage coordinator lock");
        endpoints.retain(|endpoint| endpoint.strong_count() > 0);
        let snapshot = self.local.snapshot(url)?;
        let endpoint = Arc::new(Mutex::new(Endpoint {
            origin,
            queue: VecDeque::new(),
            bytes: 0,
            sequences: [0; 2],
            wake: None,
            error: None,
        }));
        endpoints.push(Arc::downgrade(&endpoint));
        Ok((snapshot, StorageSubscription(endpoint)))
    }

    /// Returns false without changing anything when a recipient needs to drain.
    /// Each source's FIFO sequence is validated separately from the origin version:
    /// stale cached versions are not compare-and-swap operations in Web Storage.
    pub fn apply(
        &self,
        source: &StorageSubscription,
        writes: &[StorageWrite],
        session: &mut SessionStorage,
    ) -> Result<bool, StorageError> {
        let Some(first) = writes.first() else {
            return Ok(true);
        };
        let mut endpoints = self.endpoints.lock().expect("storage coordinator lock");
        endpoints.retain(|endpoint| endpoint.strong_count() > 0);
        if !endpoints
            .iter()
            .any(|endpoint| endpoint.ptr_eq(&Arc::downgrade(&source.0)))
        {
            return Err(StorageError::Invalid("retired storage source"));
        }
        let area = first.mutation.area;
        let slot = usize::from(area == StorageAreaKind::Session);
        let (origin, mut sequence) = {
            let endpoint = source.0.lock().expect("storage endpoint lock");
            (endpoint.origin.clone(), endpoint.sequences[slot])
        };
        for write in writes {
            write.validate()?;
            sequence = sequence
                .checked_add(1)
                .ok_or(StorageError::Invalid("storage sequence exhausted"))?;
            if write.sequence != sequence
                || write.mutation.area != area
                || storage_origin(&write.source_url)? != origin
            {
                return Err(StorageError::Invalid("storage source identity or sequence"));
            }
        }
        let snapshot = match area {
            StorageAreaKind::Local => self.local.snapshot(&first.source_url)?,
            StorageAreaKind::Session => session.snapshot(&first.source_url)?,
        };
        let original_version = snapshot.version;
        let mut state = StorageAreaState::from_snapshot(snapshot)?;
        let mut mutations = Vec::with_capacity(writes.len());
        let mut changes = Vec::with_capacity(writes.len());
        let mut rejection = None;
        for write in writes {
            let mutation = StorageMutation {
                expected_version: state.version(),
                ..write.mutation.clone()
            };
            let mut change = StorageChange::before(Some(&state), &mutation.operation);
            match state.apply(&mutation) {
                Ok(true) => {
                    change.version = state.version();
                    changes.push(Some(change));
                }
                Ok(false) => changes.push(None),
                Err(error) => {
                    rejection = Some(error.to_string());
                    break;
                }
            }
            mutations.push(mutation);
        }
        if rejection.is_some() {
            changes = vec![None; writes.len()];
        }
        let targets = endpoints
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|endpoint| {
                Arc::ptr_eq(endpoint, &source.0)
                    || (area == StorageAreaKind::Local
                        && endpoint.lock().expect("storage endpoint lock").origin == origin)
            })
            .collect::<Vec<_>>();
        let make_updates = |is_source: bool, changes: &[Option<StorageChange>]| {
            let mut version = original_version;
            writes
                .iter()
                .zip(changes)
                .filter_map(|(write, change)| {
                    if let Some(change) = change {
                        version = change.version;
                    }
                    (is_source || change.is_some()).then(|| StorageUpdate {
                        area,
                        version,
                        acknowledgement: if is_source { write.sequence } else { 0 },
                        change: change.clone(),
                        source_url: write.source_url.clone(),
                    })
                })
                .collect::<Vec<_>>()
        };
        // Reserve by inspecting exact records (including old values), before persistence.
        for target in &targets {
            let updates = make_updates(Arc::ptr_eq(target, &source.0), &changes);
            let bytes = updates.iter().map(StorageUpdate::byte_len).sum::<usize>();
            if updates.len() > crate::limits::MAX_QUEUED_BROWSER_WRITES
                || bytes > crate::limits::MAX_PENDING_STORAGE_BYTES
            {
                return Err(StorageError::Invalid(
                    "storage transaction exceeds delivery capacity",
                ));
            }
            let endpoint = target.lock().expect("storage endpoint lock");
            if endpoint.queue.len() + updates.len() > crate::limits::MAX_QUEUED_BROWSER_WRITES
                || endpoint.bytes + bytes > crate::limits::MAX_PENDING_STORAGE_BYTES
            {
                return Ok(false);
            }
        }
        if rejection.is_none() {
            let result = match area {
                StorageAreaKind::Local => self.local.apply_batch(&first.source_url, &mutations),
                StorageAreaKind::Session => {
                    // Preflight succeeded above; publish the whole validated map at once.
                    session.origins.insert(origin, state);
                    Ok(true)
                }
            };
            if let Err(error) = result {
                rejection = Some(error.to_string());
                changes.fill(None);
            }
        }
        let mut wakes = Vec::new();
        for target in targets {
            let is_source = Arc::ptr_eq(&target, &source.0);
            let updates = make_updates(is_source, &changes);
            let mut endpoint = target.lock().expect("storage endpoint lock");
            if is_source {
                endpoint.sequences[slot] = sequence;
                endpoint.error = rejection.clone();
            }
            if updates.is_empty() {
                continue;
            }
            endpoint.bytes += updates.iter().map(StorageUpdate::byte_len).sum::<usize>();
            endpoint.queue.extend(updates);
            if let Some(wake) = &endpoint.wake {
                wakes.push(Arc::clone(wake));
            }
        }
        drop(endpoints);
        for wake in wakes {
            wake();
        }
        Ok(true)
    }
}
