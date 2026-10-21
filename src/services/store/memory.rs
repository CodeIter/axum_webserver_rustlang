//! In-memory key-value store.
//!
//! Entries live in a `BTreeMap` (so listing is ordered by key) behind a
//! `RwLock`. The lock is only held for short, non-blocking sections and never
//! across an `.await`. The number of entries is capped to bound memory use.

use std::{
    collections::BTreeMap,
    sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use chrono::Utc;
use serde_json::Value;

use super::{validate_key, validate_value, KeyValueStore, Record, StoreError};

/// Process-local store; contents are lost on restart.
pub struct MemoryStore {
    max_entries: usize,
    entries: RwLock<BTreeMap<String, Record>>,
}

impl MemoryStore {
    /// Creates an empty store holding at most `max_entries` entries.
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: RwLock::new(BTreeMap::new()),
        }
    }

    // Every critical section leaves the map consistent, so a poisoned lock is safe to reuse.
    fn read(&self) -> RwLockReadGuard<'_, BTreeMap<String, Record>> {
        self.entries.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, BTreeMap<String, Record>> {
        self.entries.write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl KeyValueStore for MemoryStore {
    async fn create(&self, key: &str, value: Value) -> Result<Record, StoreError> {
        validate_key(key)?;
        validate_value(&value)?;
        let mut entries = self.write();
        if entries.contains_key(key) {
            return Err(StoreError::AlreadyExists);
        }
        if entries.len() >= self.max_entries {
            return Err(StoreError::CapacityExceeded);
        }
        let now = Utc::now();
        let record = Record {
            key: key.to_owned(),
            value,
            created_at: now,
            updated_at: now,
        };
        entries.insert(key.to_owned(), record.clone());
        Ok(record)
    }

    async fn get(&self, key: &str) -> Result<Option<Record>, StoreError> {
        validate_key(key)?;
        Ok(self.read().get(key).cloned())
    }

    async fn update(&self, key: &str, value: Value) -> Result<Record, StoreError> {
        validate_key(key)?;
        validate_value(&value)?;
        let mut entries = self.write();
        let record = entries.get_mut(key).ok_or(StoreError::NotFound)?;
        record.value = value;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        validate_key(key)?;
        self.write()
            .remove(key)
            .map(|_| ())
            .ok_or(StoreError::NotFound)
    }

    async fn list(&self, limit: usize, offset: usize) -> Result<Vec<Record>, StoreError> {
        Ok(self
            .read()
            .values()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn full_crud_cycle() {
        let store = MemoryStore::new(10);
        let created = store
            .create("greeting", json!({"text": "hi"}))
            .await
            .unwrap();
        assert_eq!(created.created_at, created.updated_at);

        assert_eq!(
            store.get("greeting").await.unwrap().unwrap().value,
            json!({"text": "hi"})
        );
        assert!(store.get("missing").await.unwrap().is_none());

        let updated = store.update("greeting", json!([1, 2, 3])).await.unwrap();
        assert_eq!(updated.value, json!([1, 2, 3]));
        assert_eq!(updated.created_at, created.created_at);
        assert!(updated.updated_at >= created.updated_at);

        store.delete("greeting").await.unwrap();
        assert!(store.get("greeting").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn enforces_semantics() {
        let store = MemoryStore::new(2);
        store.create("a", json!(1)).await.unwrap();
        assert!(matches!(
            store.create("a", json!(2)).await,
            Err(StoreError::AlreadyExists)
        ));
        assert!(matches!(
            store.update("nope", json!(1)).await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            store.delete("nope").await,
            Err(StoreError::NotFound)
        ));
        store.create("b", json!(1)).await.unwrap();
        assert!(matches!(
            store.create("c", json!(1)).await,
            Err(StoreError::CapacityExceeded)
        ));
        // Updating and deleting still work at capacity, and deleting frees a slot.
        store.update("a", json!(9)).await.unwrap();
        store.delete("a").await.unwrap();
        store.create("c", json!(1)).await.unwrap();
    }

    #[tokio::test]
    async fn validates_input() {
        let store = MemoryStore::new(10);
        assert!(matches!(
            store.create("", json!(1)).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            store.create("a\nb", json!(1)).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            store.create(&"k".repeat(300), json!(1)).await,
            Err(StoreError::Invalid(_))
        ));
        let big = json!("x".repeat(super::super::MAX_VALUE_BYTES + 1));
        assert!(matches!(
            store.create("big", big).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn list_is_ordered_and_paginated() {
        let store = MemoryStore::new(10);
        for key in ["c", "a", "d", "b"] {
            store.create(key, json!(key)).await.unwrap();
        }
        let keys = |records: Vec<Record>| records.into_iter().map(|r| r.key).collect::<Vec<_>>();
        assert_eq!(keys(store.list(10, 0).await.unwrap()), ["a", "b", "c", "d"]);
        assert_eq!(keys(store.list(2, 1).await.unwrap()), ["b", "c"]);
        assert!(store.list(5, 99).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn concurrent_creates_respect_the_limit() {
        let store = Arc::new(MemoryStore::new(25));
        let tasks: Vec<_> = (0..100)
            .map(|i| {
                let store = Arc::clone(&store);
                tokio::spawn(
                    async move { store.create(&format!("key-{i}"), json!(i)).await.is_ok() },
                )
            })
            .collect();
        let mut created = 0;
        for task in tasks {
            created += usize::from(task.await.unwrap());
        }
        assert_eq!(created, 25);
        assert_eq!(store.list(1000, 0).await.unwrap().len(), 25);
    }
}
