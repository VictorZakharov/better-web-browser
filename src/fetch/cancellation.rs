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

#[derive(Debug, Clone)]
pub struct FetchSignal {
    aborted: Vec<Arc<AtomicBool>>,
}

impl FetchController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn signal(&self) -> FetchSignal {
        FetchSignal {
            aborted: vec![Arc::clone(&self.aborted)],
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
        self.aborted
            .iter()
            .any(|aborted| aborted.load(Ordering::Acquire))
    }

    pub fn any(&self, other: &Self) -> Self {
        let mut aborted = self.aborted.clone();
        aborted.extend(other.aborted.iter().cloned());
        Self { aborted }
    }

    pub fn check(&self) -> Result<(), FetchError> {
        if self.is_aborted() {
            Err(FetchError::aborted())
        } else {
            Ok(())
        }
    }
}

impl Default for FetchSignal {
    fn default() -> Self {
        FetchController::new().signal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_signal_observes_either_controller() {
        let document = FetchController::new();
        let request = FetchController::new();
        let combined = document.signal().any(&request.signal());
        assert!(!combined.is_aborted());
        request.abort();
        assert!(combined.is_aborted());
    }
}
