//! Cooperative cancellation shared by every request owned by one document.

use super::FetchError;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct FetchController {
    aborted: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Default)]
pub struct FetchSignal {
    aborted: Arc<AtomicBool>,
}

impl FetchController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn signal(&self) -> FetchSignal {
        FetchSignal {
            aborted: Arc::clone(&self.aborted),
        }
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::Acquire)
    }
}

impl FetchSignal {
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), FetchError> {
        if self.is_aborted() {
            Err(FetchError::aborted())
        } else {
            Ok(())
        }
    }
}
