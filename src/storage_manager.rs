//! WHATWG StorageManager's browser-owned, rough default-bucket accounting.
//!
//! https://storage.spec.whatwg.org/#usage-and-quota
//! The quota is a fixed logical aggregate, not derived from free disk space
//! and not a promise that this much disk is reservable. Each constituent store
//! independently enforces its own per-origin/global cap: localStorage 5 MiB,
//! IndexedDB 16 MiB, and CacheStorage 16 MiB. Session storage is excluded.
//! Reported usage is deliberately rough, based on each store's quota units.

use crate::cache_storage::CacheStorage;
use crate::indexed_db::IndexedDb;
use crate::storage::LocalStorage;
use serde::Serialize;

pub const DEFAULT_BUCKET_QUOTA: u64 = 37 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StorageEstimate {
    pub usage: u64,
    pub quota: u64,
}

impl StorageEstimate {
    /// The caller must supply the committed client's origin, never a renderer URL.
    /// Each store reports its own approximate serialized payload usage.
    pub fn for_origin(
        origin: &str,
        local_storage: &LocalStorage,
        indexed_db: &IndexedDb,
        cache_storage: &CacheStorage,
    ) -> Result<Self, String> {
        let local = local_storage
            .usage(origin)
            .map_err(|error| error.to_string())?;
        let indexed = indexed_db
            .usage(origin)
            .map_err(|error| error.to_string())?;
        let cache = cache_storage
            .usage(origin)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            usage: local.saturating_add(indexed).saturating_add(cache),
            quota: DEFAULT_BUCKET_QUOTA,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_storage::CacheCommand;
    use crate::indexed_db::StoreDefinition;
    use crate::storage::{StorageAreaKind, StorageMutation, StorageOperation, StorageString};

    const FIRST: &str = "https://first.example";
    const SECOND: &str = "https://second.example";

    fn estimate(
        origin: &str,
        local: &LocalStorage,
        indexed: &IndexedDb,
        cache: &CacheStorage,
    ) -> StorageEstimate {
        StorageEstimate::for_origin(origin, local, indexed, cache).unwrap()
    }

    #[test]
    fn estimates_actual_bytes_from_all_three_origin_partitioned_stores() {
        let local = LocalStorage::in_memory();
        let indexed = IndexedDb::in_memory();
        let cache = CacheStorage::in_memory();
        let empty = estimate(FIRST, &local, &indexed, &cache);
        assert_eq!(empty.usage, 0);
        assert_eq!(empty.quota, DEFAULT_BUCKET_QUOTA);

        local
            .apply(
                FIRST,
                &StorageMutation {
                    area: StorageAreaKind::Local,
                    expected_version: 1,
                    operation: StorageOperation::Set {
                        key: StorageString::from_units("key".encode_utf16().collect()),
                        value: StorageString::from_units("value".encode_utf16().collect()),
                    },
                },
            )
            .unwrap();
        let after_local = estimate(FIRST, &local, &indexed, &cache).usage;
        assert_eq!(after_local, 8);
        indexed
            .upgrade(
                FIRST,
                "records",
                0,
                1,
                &[StoreDefinition {
                    name: "items".into(),
                    key_path: None,
                    auto_increment: false,
                    indexes: Vec::new(),
                }],
                &[],
                &[],
            )
            .unwrap();
        let after_indexed = estimate(FIRST, &local, &indexed, &cache).usage;
        assert!(after_indexed > after_local);
        cache
            .execute(
                FIRST,
                CacheCommand::Open {
                    name: "assets".into(),
                },
            )
            .unwrap();
        let after_cache = estimate(FIRST, &local, &indexed, &cache).usage;
        assert!(after_cache > after_indexed);
        let total = local.usage(FIRST).unwrap()
            + indexed.usage(FIRST).unwrap()
            + cache.usage(FIRST).unwrap();
        assert_eq!(estimate(FIRST, &local, &indexed, &cache).usage, total);
        assert_eq!(total, after_cache);
        assert_eq!(estimate(SECOND, &local, &indexed, &cache).usage, 0);
        assert!(total < DEFAULT_BUCKET_QUOTA);

        cache
            .execute(
                FIRST,
                CacheCommand::DeleteCache {
                    name: "assets".into(),
                },
            )
            .unwrap();
        indexed.delete(FIRST, "records").unwrap();
        let after_deletes = estimate(FIRST, &local, &indexed, &cache);
        assert_eq!(after_deletes.usage, local.usage(FIRST).unwrap());
        assert_eq!(after_deletes.quota, DEFAULT_BUCKET_QUOTA);
    }

    #[test]
    fn rejects_opaque_origins_instead_of_exposing_profile_usage() {
        let local = LocalStorage::in_memory();
        let indexed = IndexedDb::in_memory();
        let cache = CacheStorage::in_memory();
        assert!(StorageEstimate::for_origin("null", &local, &indexed, &cache).is_err());
        assert!(
            StorageEstimate::for_origin("http://example.test", &local, &indexed, &cache).is_err()
        );
    }
}
