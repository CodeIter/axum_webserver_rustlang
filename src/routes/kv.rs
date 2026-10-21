//! Key-value store routes, mounted below `/api/v1/kv/{memory,supabase}`.
//!
//! ```text
//! GET    /items            list (?limit=&offset=)
//! POST   /items            create   {"key": "...", "value": <json>}
//! GET    /items/{key}      read
//! PUT    /items/{key}      update   {"value": <json>}
//! DELETE /items/{key}      delete
//! GET    /health           (Supabase only) live health check
//! ```

use std::sync::Arc;

use axum::{routing::get, Router};

use crate::{
    handlers::kv,
    services::store::{KeyValueStore, SupabaseStore},
};

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

/// Supabase routes: CRUD plus `/health`, or a uniform `503` if not configured.
pub fn supabase_router(store: Option<Arc<SupabaseStore>>) -> Router {
    match store {
        Some(store) => {
            let health = Router::new()
                .route("/health", get(kv::supabase_health))
                .with_state(Arc::clone(&store));
            router(store).merge(health)
        }
        None => Router::new().fallback(kv::supabase_not_configured),
    }
}
