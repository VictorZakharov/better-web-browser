//! Browser-owned Cache API storage, partitioned by the committed client origin.
//!
//! CacheStorage is separate from the HTTP cache and IndexedDB. Renderer requests
//! carry only cache operations; the browser supplies the authoritative origin.

mod model;
mod persistence;

#[cfg(test)]
mod tests;

pub use model::{CacheCommand, CacheError, CacheStorage};
