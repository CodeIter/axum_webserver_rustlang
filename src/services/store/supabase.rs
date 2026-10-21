//! Supabase-backed key-value store.
//!
//! Talks to the project's PostgREST endpoint (`{url}/rest/v1/{table}`). The
//! table needs the columns `key text primary key`, `value jsonb`,
//! `created_at timestamptz` and `updated_at timestamptz`; see
//! `supabase/kv_store.sql`.
//!
//! # Availability gate
//!
//! Every operation first calls [`SupabaseStore::ensure_available`], which runs
//! [`SupabaseStore::health_check`] unless a recent result is cached (healthy:
//! 30 s, unhealthy: 5 s). If Supabase is down, operations fail fast with
//! [`StoreError::Unavailable`] instead of each waiting for a timeout. A transport
//! error or `5xx` during an operation also marks the backend unhealthy.
//!
//! The API key is sent as a sensitive header and never logged.

use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    redirect::Policy,
    Client, RequestBuilder, Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use super::{validate_key, validate_value, KeyValueStore, Record, StoreError};

/// How long a successful health check is trusted.
const HEALTHY_TTL: Duration = Duration::from_secs(30);
/// How long a failed health check is trusted (avoids hammering a dead backend).
const UNHEALTHY_TTL: Duration = Duration::from_secs(5);
/// Upper bound for establishing a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Result of a successful health check.
#[derive(Debug, Serialize)]
pub struct SupabaseHealth {
    pub status: &'static str,
    pub latency_ms: u64,
    pub table: String,
}

#[derive(Clone, Copy)]
struct HealthState {
    checked_at: Instant,
    healthy: bool,
}

/// Error body returned by PostgREST.
#[derive(Debug, Default, Deserialize)]
struct PostgrestError {
    message: Option<String>,
    code: Option<String>,
}

/// Key-value store persisted in a Supabase table.
pub struct SupabaseStore {
    client: Client,
    table: String,
    table_url: String,
    health: Mutex<Option<HealthState>>,
}

impl SupabaseStore {
    /// Builds the store; performs no network I/O.
    ///
    /// `url` is the project URL (`https://<ref>.supabase.co`), `api_key` the
    /// server-side `service_role` key, `table` the table name (letters, digits
    /// and `_` only).
    pub fn new(
        url: &str,
        api_key: &str,
        table: &str,
        timeout: Duration,
    ) -> Result<Self, StoreError> {
        let invalid = |message: &str| StoreError::Invalid(message.to_owned());

        let parsed = reqwest::Url::parse(url)
            .map_err(|err| StoreError::Invalid(format!("invalid Supabase URL: {err}")))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(invalid("Supabase URL must be an http(s) URL"));
        }
        if table.is_empty() || !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid(
                "Supabase table name may only contain letters, digits and '_'",
            ));
        }

        let sensitive = |value: String| {
            HeaderValue::from_str(&value)
                .map(|mut header| {
                    header.set_sensitive(true);
                    header
                })
                .map_err(|_| invalid("Supabase API key contains invalid characters"))
        };
        let mut headers = HeaderMap::new();
        headers.insert("apikey", sensitive(api_key.to_owned())?);
        headers.insert(AUTHORIZATION, sensitive(format!("Bearer {api_key}"))?);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .connect_timeout(timeout.min(CONNECT_TIMEOUT))
            // Never follow redirects: the credentials must only go to the configured host.
            .redirect(Policy::none())
            .user_agent(concat!("axum-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| StoreError::Backend(format!("cannot build HTTP client: {err}")))?;

        Ok(Self {
            client,
            table: table.to_owned(),
            table_url: format!("{}/rest/v1/{table}", parsed.as_str().trim_end_matches('/')),
            health: Mutex::new(None),
        })
    }

    /// Probes Supabase now (ignoring the cache) and records the outcome.
    ///
    /// Reads one row from the table, which verifies connectivity, the API key
    /// and that the table exists.
    pub async fn health_check(&self) -> Result<SupabaseHealth, StoreError> {
        let started = Instant::now();
        let probe = self
            .client
            .get(&self.table_url)
            .query(&[("select", "key"), ("limit", "1")]);
        let outcome = self.execute(probe).await;
        self.set_health(outcome.is_ok());
        outcome.map(|_| SupabaseHealth {
            status: "ok",
            latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            table: self.table.clone(),
        })
    }

    /// Fails fast when Supabase is known to be down; otherwise refreshes the
    /// health state if it is stale.
    async fn ensure_available(&self) -> Result<(), StoreError> {
        match self.cached_health() {
            Some(true) => Ok(()),
            Some(false) => Err(StoreError::Unavailable(
                "a recent Supabase health check failed".into(),
            )),
            None => self.health_check().await.map(|_| ()),
        }
    }

    fn cached_health(&self) -> Option<bool> {
        let state = (*self.health.lock().unwrap_or_else(PoisonError::into_inner))?;
        let ttl = if state.healthy {
            HEALTHY_TTL
        } else {
            UNHEALTHY_TTL
        };
        (state.checked_at.elapsed() < ttl).then_some(state.healthy)
    }

    fn set_health(&self, healthy: bool) {
        *self.health.lock().unwrap_or_else(PoisonError::into_inner) = Some(HealthState {
            checked_at: Instant::now(),
            healthy,
        });
    }

    /// Sends a request and maps transport failures and HTTP errors to [`StoreError`].
    async fn execute(&self, request: RequestBuilder) -> Result<Response, StoreError> {
        let response = request.send().await.map_err(|err| {
            self.set_health(false);
            StoreError::Unavailable(describe_transport_error(err))
        })?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let detail: PostgrestError = response.json().await.unwrap_or_default();
        let detail_text = format!(
            "HTTP {status}: {} ({})",
            detail.message.as_deref().unwrap_or("no message"),
            detail.code.as_deref().unwrap_or("no code")
        );
        match status {
            StatusCode::CONFLICT => Err(StoreError::AlreadyExists),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(StoreError::Backend(format!(
                "authentication failed; check the Supabase API key ({detail_text})"
            ))),
            StatusCode::NOT_FOUND => Err(StoreError::Backend(format!(
                "table not found; run supabase/kv_store.sql ({detail_text})"
            ))),
            s if s.is_server_error() => {
                self.set_health(false);
                Err(StoreError::Unavailable(detail_text))
            }
            _ => Err(StoreError::Backend(detail_text)),
        }
    }

    /// Decodes a PostgREST response body (an array of rows).
    async fn rows(response: Response) -> Result<Vec<Record>, StoreError> {
        response.json::<Vec<Record>>().await.map_err(|err| {
            StoreError::Backend(format!("unexpected response: {}", err.without_url()))
        })
    }

    /// `?key=eq.<key>`: reqwest percent-encodes the value.
    fn key_filter(key: &str) -> [(&'static str, String); 1] {
        [("key", format!("eq.{key}"))]
    }
}

/// Describes a transport error without leaking the request URL.
fn describe_transport_error(err: reqwest::Error) -> String {
    let kind = if err.is_timeout() {
        "request timed out"
    } else if err.is_connect() {
        "connection failed"
    } else {
        "request failed"
    };
    warn!(error = %err.without_url(), "Supabase {kind}");
    kind.to_owned()
}

impl KeyValueStore for SupabaseStore {
    async fn create(&self, key: &str, value: Value) -> Result<Record, StoreError> {
        validate_key(key)?;
        validate_value(&value)?;
        self.ensure_available().await?;
        let request = self
            .client
            .post(&self.table_url)
            .header("Prefer", "return=representation")
            .json(&json!({ "key": key, "value": value }));
        let rows = Self::rows(self.execute(request).await?).await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| StoreError::Backend("insert returned no row".into()))
    }

    async fn get(&self, key: &str) -> Result<Option<Record>, StoreError> {
        validate_key(key)?;
        self.ensure_available().await?;
        let request = self
            .client
            .get(&self.table_url)
            .query(&Self::key_filter(key))
            .query(&[("select", "*"), ("limit", "1")]);
        Ok(Self::rows(self.execute(request).await?)
            .await?
            .into_iter()
            .next())
    }

    async fn update(&self, key: &str, value: Value) -> Result<Record, StoreError> {
        validate_key(key)?;
        validate_value(&value)?;
        self.ensure_available().await?;
        let request = self
            .client
            .patch(&self.table_url)
            .query(&Self::key_filter(key))
            .header("Prefer", "return=representation")
            .json(&json!({ "value": value, "updated_at": Utc::now().to_rfc3339() }));
        let rows = Self::rows(self.execute(request).await?).await?;
        rows.into_iter().next().ok_or(StoreError::NotFound)
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        validate_key(key)?;
        self.ensure_available().await?;
        let request = self
            .client
            .delete(&self.table_url)
            .query(&Self::key_filter(key))
            .header("Prefer", "return=representation");
        // The representation tells us whether a row was actually deleted.
        let rows = Self::rows(self.execute(request).await?).await?;
        if rows.is_empty() {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn list(&self, limit: usize, offset: usize) -> Result<Vec<Record>, StoreError> {
        self.ensure_available().await?;
        let request = self.client.get(&self.table_url).query(&[
            ("select", "*".to_owned()),
            ("order", "key.asc".to_owned()),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ]);
        Self::rows(self.execute(request).await?).await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use axum::{
        body::Bytes,
        extract::{Query, State},
        http::Method,
        response::{IntoResponse, Response},
        routing::any,
        Json, Router,
    };

    use super::*;

    const KEY: &str = "secret";

    /// Minimal PostgREST imitation: one table, `key=eq.` filters, `Prefer: return=representation`.
    #[derive(Clone, Default)]
    struct Fake {
        rows: Arc<Mutex<BTreeMap<String, Value>>>,
        probes: Arc<AtomicUsize>,
    }

    async fn rest(
        State(fake): State<Fake>,
        method: Method,
        headers: axum::http::HeaderMap,
        Query(query): Query<HashMap<String, String>>,
        body: Bytes,
    ) -> Response {
        let authorized = headers.get("apikey").is_some_and(|v| v == KEY)
            && headers
                .get("authorization")
                .is_some_and(|v| v == "Bearer secret");
        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"message": "Invalid API key"})),
            )
                .into_response();
        }
        if method == Method::GET && query.get("select").map(String::as_str) == Some("key") {
            fake.probes.fetch_add(1, Ordering::SeqCst);
        }

        let target = query
            .get("key")
            .and_then(|v| v.strip_prefix("eq."))
            .map(str::to_owned);
        let now = Utc::now().to_rfc3339();
        let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let mut rows = fake.rows.lock().unwrap();

        match (method, target) {
            (Method::POST, _) => {
                let key = body["key"].as_str().unwrap().to_owned();
                if rows.contains_key(&key) {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({"code": "23505", "message": "duplicate key"})),
                    )
                        .into_response();
                }
                let row = json!({"key": key, "value": body["value"], "created_at": now, "updated_at": now});
                rows.insert(key, row.clone());
                (StatusCode::CREATED, Json(json!([row]))).into_response()
            }
            (Method::GET, Some(key)) => {
                Json(rows.get(&key).cloned().into_iter().collect::<Vec<_>>()).into_response()
            }
            (Method::GET, None) => {
                let number = |name: &str, default: usize| {
                    query
                        .get(name)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(default)
                };
                let page: Vec<Value> = rows
                    .values()
                    .skip(number("offset", 0))
                    .take(number("limit", usize::MAX))
                    .cloned()
                    .collect();
                Json(page).into_response()
            }
            (Method::PATCH, Some(key)) => {
                let updated = rows.get_mut(&key).map(|row| {
                    row["value"] = body["value"].clone();
                    row["updated_at"] = body["updated_at"].clone();
                    row.clone()
                });
                Json(updated.into_iter().collect::<Vec<_>>()).into_response()
            }
            (Method::DELETE, Some(key)) => {
                Json(rows.remove(&key).into_iter().collect::<Vec<_>>()).into_response()
            }
            _ => StatusCode::BAD_REQUEST.into_response(),
        }
    }

    async fn spawn_fake() -> (String, Fake) {
        let fake = Fake::default();
        let app = Router::new()
            .route("/rest/v1/kv_store", any(rest))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, fake)
    }

    fn store(url: &str, key: &str) -> SupabaseStore {
        SupabaseStore::new(url, key, "kv_store", Duration::from_secs(5)).unwrap()
    }

    #[tokio::test]
    async fn full_crud_cycle_against_fake_postgrest() {
        let (url, _) = spawn_fake().await;
        let store = store(&url, KEY);

        let created = store
            .create("user:1", json!({"name": "Ada"}))
            .await
            .unwrap();
        assert_eq!(created.key, "user:1");
        assert!(matches!(
            store.create("user:1", json!(1)).await,
            Err(StoreError::AlreadyExists)
        ));

        // Keys with reserved URL characters must round-trip.
        let odd = "a b&c=d/é";
        store.create(odd, json!(true)).await.unwrap();
        assert_eq!(store.get(odd).await.unwrap().unwrap().value, json!(true));

        assert_eq!(
            store.get("user:1").await.unwrap().unwrap().value,
            json!({"name": "Ada"})
        );
        assert!(store.get("missing").await.unwrap().is_none());

        let updated = store
            .update("user:1", json!({"name": "Grace"}))
            .await
            .unwrap();
        assert_eq!(updated.value, json!({"name": "Grace"}));
        assert!(matches!(
            store.update("missing", json!(1)).await,
            Err(StoreError::NotFound)
        ));

        let keys: Vec<String> = store
            .list(10, 0)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.key)
            .collect();
        assert_eq!(keys, [odd, "user:1"]);
        assert_eq!(store.list(1, 1).await.unwrap().len(), 1);

        store.delete("user:1").await.unwrap();
        assert!(matches!(
            store.delete("user:1").await,
            Err(StoreError::NotFound)
        ));
        assert!(store.get("user:1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn health_check_reports_latency_and_gates_operations_with_caching() {
        let (url, fake) = spawn_fake().await;
        let store = store(&url, KEY);

        let health = store.health_check().await.unwrap();
        assert_eq!((health.status, health.table.as_str()), ("ok", "kv_store"));
        assert_eq!(fake.probes.load(Ordering::SeqCst), 1);

        // The fresh result is cached: operations do not probe again.
        store.create("a", json!(1)).await.unwrap();
        store.get("a").await.unwrap();
        assert_eq!(fake.probes.load(Ordering::SeqCst), 1);

        // A brand-new store has no cached state, so its first operation probes first.
        let fresh = self::store(&url, KEY);
        fresh.get("a").await.unwrap();
        assert_eq!(fake.probes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn unreachable_backend_fails_fast_and_is_cached() {
        // Nothing listens on port 1.
        let store = store("http://127.0.0.1:1", KEY);
        assert!(matches!(
            store.health_check().await,
            Err(StoreError::Unavailable(_))
        ));
        // Subsequent operations are rejected from the cached failure.
        assert!(matches!(
            store.create("a", json!(1)).await,
            Err(StoreError::Unavailable(_))
        ));
        assert!(matches!(
            store.get("a").await,
            Err(StoreError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn wrong_api_key_is_reported_as_backend_error() {
        let (url, _) = spawn_fake().await;
        let store = store(&url, "wrong");
        let err = store.health_check().await.unwrap_err();
        assert!(
            matches!(&err, StoreError::Backend(m) if m.contains("authentication failed")),
            "{err}"
        );
        // The failed probe also blocks operations until the cache expires.
        assert!(matches!(
            store.get("a").await,
            Err(StoreError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn input_is_validated_before_any_network_call() {
        let store = store("http://127.0.0.1:1", KEY);
        // Would be `Unavailable` if it reached the network.
        assert!(matches!(store.get("").await, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn constructor_validates_configuration() {
        let new =
            |url: &str, table: &str| SupabaseStore::new(url, KEY, table, Duration::from_secs(1));
        assert!(new("https://abc.supabase.co", "kv_store").is_ok());
        assert!(new("ftp://abc.supabase.co", "kv_store").is_err());
        assert!(new("not a url", "kv_store").is_err());
        assert!(new("https://abc.supabase.co", "kv store; drop").is_err());
        assert!(new("https://abc.supabase.co", "").is_err());
        assert!(SupabaseStore::new(
            "https://abc.supabase.co",
            "bad\nkey",
            "t",
            Duration::from_secs(1)
        )
        .is_err());
        assert_eq!(
            new("https://abc.supabase.co/", "kv_store")
                .unwrap()
                .table_url,
            "https://abc.supabase.co/rest/v1/kv_store"
        );
    }
}
