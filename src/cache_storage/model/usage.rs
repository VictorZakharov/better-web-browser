//! Per-origin estimate using the same serialized accounting as the quota check.

use super::*;

impl CacheStorage {
    pub fn usage(&self, origin: &str) -> Result<u64, CacheError> {
        let parsed = Url::parse(origin).map_err(|_| CacheError::InvalidRequest)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.origin().ascii_serialization() != origin
            || !crate::fetch::Origin::parse(origin)
                .map_err(|_| CacheError::InvalidRequest)?
                .is_potentially_trustworthy()
        {
            return Err(CacheError::InvalidRequest);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| CacheError::Persistence("storage lock poisoned".into()))?;
        let state = self.ensure_loaded(&mut state)?;
        state
            .origins
            .get(origin)
            .filter(|caches| !caches.is_empty())
            .map_or(Ok(0), |caches| {
                serde_json::to_vec(caches)
                    .map(|bytes| bytes.len() as u64)
                    .map_err(|error| CacheError::Persistence(error.to_string()))
            })
    }
}
