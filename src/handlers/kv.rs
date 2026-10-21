//! HTTP handlers for the key-value stores.
//!
//! The CRUD handlers are generic over [`KeyValueStore`], so the exact same code
//! serves the in-memory and the Supabase store; each store gets its own router
//! whose state is that store.

use std::sync::Arc;

use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::{json_error, ApiError},
    services::store::{KeyValueStore, Record, StoreError, SupabaseStore},
};

const DEFAULT_LIST_LIMIT: usize = 50;
const MAX_LIST_LIMIT: usize = 200;

/// Body of `POST .../items`.
#[derive(Deserialize)]
pub struct CreateItem {
    key: String,
    value: Value,
}

/// Body of `PUT .../items/{key}`.
#[derive(Deserialize)]
pub struct UpdateItem {
    value: Value,
}

/// Query string of `GET .../items`.
#[derive(Deserialize)]
pub struct ListQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
pub struct ListResponse {
    items: Vec<Record>,
    limit: usize,
    offset: usize,
}

/// `POST .../items` - creates an entry (`201`, or `409` if the key exists).
pub async fn create<S: KeyValueStore>(
    State(store): State<Arc<S>>,
    body: Result<Json<CreateItem>, JsonRejection>,
) -> Result<(StatusCode, Json<Record>), ApiError> {
    let Json(CreateItem { key, value }) = body?;
    let record = store.create(&key, value).await?;
    Ok((StatusCode::CREATED, Json(record)))
}

/// `GET .../items/{key}`
pub async fn read<S: KeyValueStore>(
    State(store): State<Arc<S>>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<Record>, ApiError> {
    let Path(key) = path?;
    store
        .get(&key)
        .await?
        .map(Json)
        .ok_or(ApiError::from(StoreError::NotFound))
}

/// `PUT .../items/{key}` - replaces the value of an existing entry (`404` if absent).
pub async fn update<S: KeyValueStore>(
    State(store): State<Arc<S>>,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<UpdateItem>, JsonRejection>,
) -> Result<Json<Record>, ApiError> {
    let Path(key) = path?;
    let Json(UpdateItem { value }) = body?;
    Ok(Json(store.update(&key, value).await?))
}

/// `DELETE .../items/{key}` - `204` on success, `404` if absent.
pub async fn delete<S: KeyValueStore>(
    State(store): State<Arc<S>>,
    path: Result<Path<String>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let Path(key) = path?;
    store.delete(&key).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET .../items?limit=&offset=` - entries ordered by key.
pub async fn list<S: KeyValueStore>(
    State(store): State<Arc<S>>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<ListResponse>, ApiError> {
    let Query(ListQuery { limit, offset }) = query?;
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
    let offset = offset.unwrap_or(0);
    let items = store.list(limit, offset).await?;
    Ok(Json(ListResponse {
        items,
        limit,
        offset,
    }))
}

/// `GET /api/v1/kv/supabase/health` - probes Supabase right now.
pub async fn supabase_health(State(store): State<Arc<SupabaseStore>>) -> Response {
    match store.health_check().await {
        Ok(health) => Json(health).into_response(),
        Err(StoreError::Backend(detail)) => {
            tracing::warn!(%detail, "Supabase health probe rejected");
            json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Supabase is reachable but rejected the probe (check SUPABASE_KEY and the table)",
            )
        }
        Err(err) => {
            tracing::warn!(%err, "Supabase health probe failed");
            json_error(StatusCode::SERVICE_UNAVAILABLE, "Supabase is unreachable")
        }
    }
}

/// Fallback when the Supabase store is not configured.
pub async fn supabase_not_configured() -> ApiError {
    ApiError::ServiceUnavailable(
        "Supabase store is not configured (set SUPABASE_URL and SUPABASE_KEY)".into(),
    )
}

/// Fallback for unknown paths below a store prefix.
pub async fn unknown_endpoint() -> Response {
    json_error(StatusCode::NOT_FOUND, "no such API endpoint")
}
