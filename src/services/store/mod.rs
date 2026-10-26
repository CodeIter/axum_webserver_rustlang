//! Key-value storage abstraction with two backends:
//!
//! * [`MemoryStore`]: process-local, bounded, lost on restart.
//!
//! Both implement [`KeyValueStore`] with identical semantics, so callers can be
//! generic over the backend. Values are arbitrary JSON.

pub mod memory;

use std::future::Future;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use memory::MemoryStore;

/// Longest accepted key, in characters.
pub const MAX_KEY_LEN: usize = 256;
/// Largest accepted value, as serialized JSON.
pub const MAX_VALUE_BYTES: usize = 64 * 1024;

/// A stored entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Failure of a store operation.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The key or value is not acceptable (client error).
    #[error("{0}")]
    Invalid(String),
    #[error("key not found")]
    NotFound,
    #[error("key already exists")]
    AlreadyExists,
    /// The in-memory store reached its entry limit.
    #[error("store capacity reached")]
    CapacityExceeded,
}

/// CRUD operations shared by every store.
///
/// Semantics: `create` fails with [`StoreError::AlreadyExists`] if the key is
/// taken; `update` and `delete` fail with [`StoreError::NotFound`] if it is
/// missing (there is no implicit upsert).
pub trait KeyValueStore: Send + Sync + 'static {
    /// Inserts a new entry.
    fn create(
        &self,
        key: &str,
        value: Value,
    ) -> impl Future<Output = Result<Record, StoreError>> + Send;
    /// Returns the entry, or `None` if the key does not exist.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<Record>, StoreError>> + Send;
    /// Replaces the value of an existing entry.
    fn update(
        &self,
        key: &str,
        value: Value,
    ) -> impl Future<Output = Result<Record, StoreError>> + Send;
    /// Removes an existing entry.
    fn delete(&self, key: &str) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Lists entries ordered by key.
    fn list(
        &self,
        limit: usize,
        offset: usize,
    ) -> impl Future<Output = Result<Vec<Record>, StoreError>> + Send;
}

/// Keys must be 1..=[`MAX_KEY_LEN`] characters without control characters.
pub fn validate_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty() || key.chars().count() > MAX_KEY_LEN {
        return Err(StoreError::Invalid(format!(
            "key must be between 1 and {MAX_KEY_LEN} characters"
        )));
    }
    if key.chars().any(char::is_control) {
        return Err(StoreError::Invalid(
            "key must not contain control characters".into(),
        ));
    }
    Ok(())
}

/// Values must serialize to at most [`MAX_VALUE_BYTES`] bytes.
pub fn validate_value(value: &Value) -> Result<(), StoreError> {
    // Serializing a `Value` cannot fail; a failure would only mean "too big".
    let size = serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len());
    if size > MAX_VALUE_BYTES {
        return Err(StoreError::Invalid(format!(
            "value must be at most {MAX_VALUE_BYTES} bytes of JSON"
        )));
    }
    Ok(())
}
