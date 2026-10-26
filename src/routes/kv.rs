//! Key-value store routes, mounted below `/api/v1/kv/memory`.
//!
//! ```text
//! GET    /items            list (?limit=&offset=)
//! POST   /items            create   {"key": "...", "value": <json>}
//! GET    /items/{key}      read
//! PUT    /items/{key}      update   {"value": <json>}
//! DELETE /items/{key}      delete
//! ```

use std::sync::Arc;

use axum::{routing::get, Router};

use crate::{handlers::kv, services::store::KeyValueStore};

/// CRUD router for any store; the store itself is the router state.
pub fn router<S: KeyValueStore>(store: Arc<S>) -> Router {
    Router::new()
        .route("/items", get(kv::list::<S>).post(kv::create::<S>))
        .route(
            "/items/{key}",
            get(kv::read::<S>)
                .put(kv::update::<S>)
                .delete(kv::delete::<S>),
        )
        .fallback(kv::unknown_endpoint)
        .with_state(store)
}
