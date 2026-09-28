//! Per-origin estimate using the same serialized accounting as the quota check.

use super::*;

impl IndexedDb {
    pub fn usage(&self, url: &str) -> Result<u64, DbError> {
        let origin = origin(url)?;
        let state = self
            .state
            .lock()
            .map_err(|_| DbError::Persistence("database lock poisoned".into()))?;
        state
            .origins
            .get(&origin)
            .filter(|group| !group.is_empty())
            .map_or(Ok(0), |group| {
                serde_json::to_vec(group)
                    .map(|bytes| bytes.len() as u64)
                    .map_err(|error| DbError::Persistence(error.to_string()))
            })
    }
}
