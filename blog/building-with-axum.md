---
slug: building-with-axum
title: Building a web server with Axum
excerpt: A tour of the router, extractors and shared state in an Axum 0.8 server.
author: [CodeIter]
license: CC BY 4.0
createdAt: 2026-10-27T16:15:15Z
updatedAt: 2026-10-27T16:15:15Z
keywords: rust, axum, web
category: Rust
serie: Axum Basics
lang: en
featured: false
published: true
image:
imageAlt:
canonical_url:
---
Axum is a web framework built on `tokio`, `tower` and `hyper`. It has no macros for routing and no custom runtime. You write ordinary async functions, and the type system does the wiring. In this post we build a small JSON API step by step and look at the three ideas that carry most of the framework: the **router**, **extractors** and **shared state**.

## Setup

Create a project and add the dependencies:

```bash
cargo new axum-demo
cd axum-demo
cargo add axum@0.8
cargo add tokio --features full
cargo add serde --features derive
```

## Your first handler

A handler is an async function that returns something implementing `IntoResponse`. Strings, status codes, tuples and `Json<T>` all qualify.

```rust
async fn hello() -> &'static str {
    "Hello World"
}
```

> Handlers are plain async functions.

That sentence is the core of the design. There is no trait to implement and no base class to extend. If the function's arguments are extractors and its return type is a response, it is a handler.

## The router

The `Router` maps paths and HTTP methods to handlers. Serving it takes a `tokio` listener and `axum::serve`:

```rust
use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(hello));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
```

Run `cargo run` and visit `http://localhost:3000`. You should see `Hello World`.

Several methods can share one path by chaining them, and routes can be grouped with `nest`:

```rust
use axum::routing::{get, post};

let users = Router::new()
    .route("/", get(list_users).post(create_user))
    .route("/{id}", get(get_user));

let app = Router::new().nest("/users", users);
```

> **Axum 0.8 note:** path parameters now use curly braces (`/{id}`), not the colon syntax (`/:id`) from earlier versions. A literal brace is written `{{`.

## Extractors

Extractors parse data out of the incoming request. You declare them as function arguments and Axum runs them before calling your handler. If one fails, Axum returns an error response and your handler never runs.

| Extractor      | Purpose                                   |
|----------------|-------------------------------------------|
| `Path<T>`      | Typed values from the URL path            |
| `Query<T>`     | Typed values from the query string        |
| `Json<T>`      | Deserializes the request body as JSON     |
| `State<T>`     | Access to shared application state        |
| `HeaderMap`    | All request headers                       |

Here is a handler that combines two of them:

```rust
use axum::extract::{Path, Query};
use serde::Deserialize;

#[derive(Deserialize)]
struct Pagination {
    page: Option<u32>,
    per_page: Option<u32>,
}

async fn get_user(
    Path(id): Path<u64>,
    Query(p): Query<Pagination>,
) -> String {
    format!(
        "user {id}, page {}, per_page {}",
        p.page.unwrap_or(1),
        p.per_page.unwrap_or(20)
    )
}
```

A request to `/users/42?page=2` returns `user 42, page 2, per_page 20`. If someone requests `/users/abc`, Axum answers `400 Bad Request` because `abc` is not a `u64`.

The order of arguments matters in one case. Extractors that consume the body, such as `Json`, must come **last**, because the body can only be read once.

## Shared state

Real applications need a database pool, a config struct or a cache. Axum handles this with the `State` extractor. Define a state type, make it cheap to clone, and attach it with `with_state`.

```rust
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

#[derive(Clone, Default)]
struct AppState {
    users: Arc<RwLock<HashMap<u64, User>>>,
}
```

Because `AppState` is cloned for every request, the real data lives behind an `Arc`. A `tokio::sync::RwLock` lets many handlers read at once while writes take exclusive access.

Now the handlers:

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<NewUser>,
) -> (StatusCode, Json<User>) {
    let mut users = state.users.write().await;
    let id = users.len() as u64 + 1;
    let user = User { id, name: input.name };
    users.insert(id, user.clone());

    (StatusCode::CREATED, Json(user))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, StatusCode> {
    let users = state.users.read().await;
    users
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn list_users(State(state): State<AppState>) -> Json<Vec<User>> {
    let users = state.users.read().await;
    Json(users.values().cloned().collect())
}
```

Notice that `create_user` returns a tuple. `(StatusCode, Json<User>)` implements `IntoResponse`, so you can set the status code and body in a single expression. The `Result<Json<User>, StatusCode>` return type in `get_user` works the same way, since both arms are valid responses.

Wire it all together:

```rust
#[tokio::main]
async fn main() {
    let state = AppState::default();

    let users = Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/{id}", get(get_user));

    let app = Router::new()
        .route("/", get(hello))
        .nest("/users", users)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
```

Try it from the terminal:

```bash
curl -X POST localhost:3000/users \
  -H 'content-type: application/json' \
  -d '{"name": "Ada"}'

curl localhost:3000/users/1
```

## Error handling

Returning bare `StatusCode` values gets limiting quickly. A common pattern is a single error type that implements `IntoResponse`:

```rust
use axum::response::{IntoResponse, Response};

enum AppError {
    NotFound,
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong").into_response()
            }
        }
    }
}
```

Handlers can then return `Result<T, AppError>` and use `?` freely, as long as your error conversions are in place.

## Middleware with Tower

Because Axum is built on `tower`, every `Service` and `Layer` in that ecosystem works with it. The `tower-http` crate provides ready-made layers for tracing, CORS, compression and timeouts:

```rust
use tower_http::trace::TraceLayer;

let app = Router::new()
    .route("/", get(hello))
    .layer(TraceLayer::new_for_http());
```

A layer applied with `.layer()` wraps every route registered **before** it. Routes added afterwards are not covered, so order matters here too.

## Testing without a network

A `Router` is itself a `tower::Service`, so you can test it by calling it directly, with no port and no running server. Add `tower` (with the `util` feature) and `http-body-util` as dev-dependencies:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn hello_works() {
        let app = Router::new().route("/", get(hello));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

## Wrapping up

The pieces fit together in a few simple ways:

| Concept   | Purpose                                      |
|-----------|----------------------------------------------|
| Router    | Maps paths to handlers                       |
| Extractor | Parses request data                          |
| State     | Shares resources across requests             |
| Layer     | Adds cross-cutting behavior like logging     |

Most of what you write in Axum is ordinary Rust: functions, structs and traits.
