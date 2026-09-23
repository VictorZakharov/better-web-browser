//! Origin-partitioned IndexedDB records and atomic browser-owned transactions.
//!
//! The model is independent of the renderer and UI. Browser IPC is the only
//! way page scripts access it; this prevents renderer processes from opening
//! arbitrary profile files or observing another origin's data.

mod model;
mod persistence;
#[cfg(test)]
mod tests;

pub use model::{
    CursorRecord, DatabaseInfo, DatabaseListing, DbError, DbOperation, DbResult, DbSession,
    IndexedDb, Key, KeyRange, StoreDefinition, TransactionMode,
};
