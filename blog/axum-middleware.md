---
title: Middleware in Axum
author: CodeIter
license: CC BY 4.0
createdAt: 2026-10-27T17:15:15Z
keywords: [rust, axum, web, tower, middleware]
category: Rust
serie: Axum Basics
lang: en
featured: false
published: true
image:
imageAlt:
canonical_url:
---
Layers wrap services. Later layers run first, so order matters.

That one sentence explains most of the bugs people hit with middleware, and it is easy to get wrong in practice. In this post we look at what middleware is in Axum 0.8, how to write your own with `from_fn`, how to use the ready-made layers from `tower-http`, and how to reason about ordering so your logging, authentication and timeouts end up where you expect.

## What middleware is

Axum does not have its own middleware system. It uses `tower`, where everything is a `Service`: a thing that takes a request and asynchronously returns a response. A `Layer` is a function from one service to another, usually one that wraps the inner service and adds behavior before and after it.

Picture an onion. The handler is the core, and each layer is a skin around it. A request travels inward through every skin, reaches the handler, and the response travels back out through the same skins in reverse.

```text
request  ->  [ layer C [ layer B [ layer A [ handler ] ] ] ]  ->  response
```

Each layer can:

- inspect or modify the request before the inner service sees it,
- short-circuit and return a response without calling the inner service at all,
- inspect or modify the response on the way out.

## Setup

We will build on the API from the previous post. Add the extra crates:

```bash
cargo add tower --features util
cargo add tower-http --features trace,cors,timeout,compression-gzip
cargo add tracing tracing-subscriber
```

## Writing middleware with `from_fn`

The easiest way to write your own is `axum::middleware::from_fn`. You write an async function that takes the `Request` and a `Next`, and call `next.run(req).await` to hand control to the rest of the stack.

```rust
use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
};
use std::time::Instant;

async fn timing(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let start = Instant::now();

    let response = next.run(req).await;

    println!("{method} {path} -> {} in {:?}", response.status(), start.elapsed());
    response
}
```

Everything above `next.run` happens on the way in, and everything below it happens on the way out. Attach it with `.layer`:

```rust
let app = Router::new()
    .route("/", get(hello))
    .layer(middleware::from_fn(timing));
```

> A middleware function can also use extractors. List them before the `Request`, which must come last together with `Next`. Only the final extractor may consume the body.

## Short-circuiting: a simple auth check

A middleware does not have to call the inner service. If it returns early, the handler never runs. Here is a bearer-token check:

```rust
use axum::http::{header::AUTHORIZATION, StatusCode};

async fn require_token(req: Request, next: Next) -> Result<Response, StatusCode> {
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    if header == Some("Bearer secret") {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
```

The token is hard-coded to keep the example short. In a real application you would verify a signed token or look up a session. Returning `Result<Response, StatusCode>` works because both arms implement `IntoResponse`, the same trick we used for handlers.

## Passing data to handlers with extensions

Middleware often computes something that handlers need, such as the authenticated user or a request ID. The standard way to pass it along is the request's **extensions**, a type-keyed map.

```rust
use axum::Extension;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct RequestId(u64);

async fn add_request_id(mut req: Request, next: Next) -> Response {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    req.extensions_mut().insert(RequestId(id));
    next.run(req).await
}

async fn whoami(Extension(RequestId(id)): Extension<RequestId>) -> String {
    format!("this is request #{id}")
}
```

The handler asks for `Extension<RequestId>`, and Axum pulls it out of the request. If the middleware was not installed, the extractor fails and Axum returns a `500`, so a missing layer shows up quickly in testing.

## Middleware that needs state

A `from_fn` middleware can read the same shared state as your handlers. Use `from_fn_with_state` and pass the state in explicitly:

```rust
use axum::{extract::State, middleware::from_fn_with_state};

async fn log_user_count(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let count = state.users.read().await.len();
    println!("{count} users stored before handling {}", req.uri().path());
    next.run(req).await
}

let state = AppState::default();

let app = Router::new()
    .route("/users", get(list_users))
    .layer(from_fn_with_state(state.clone(), log_user_count))
    .with_state(state);
```

Notice that the state is passed twice. The middleware gets its own clone, and `with_state` supplies the handlers. Because `AppState` is just a few `Arc`s, the clone is cheap.

## Ready-made layers from `tower-http`

You rarely need to write everything yourself. The `tower-http` crate covers the common cases:

| Layer              | What it does                                  |
|--------------------|-----------------------------------------------|
| `TraceLayer`       | Logs requests and responses with `tracing`    |
| `CorsLayer`        | Handles CORS headers and preflight requests   |
| `CompressionLayer` | Compresses response bodies                    |
| `TimeoutLayer`     | Aborts requests that take too long            |

Using them looks the same as using your own:

```rust
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};

let app = Router::new()
    .route("/", get(hello))
    .layer(CompressionLayer::new())
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http());
```

`CorsLayer::permissive()` allows every origin, method and header. It is fine for local development, but in production you should list the origins you actually trust. Initialize a `tracing_subscriber` in `main` or you will not see anything from `TraceLayer`.

## Order matters

Here is where the opening sentence pays off. Every call to `.layer()` wraps everything that came before it, so the **last** layer you add is the **outermost** one and sees the request **first**.

```rust
async fn a(req: Request, next: Next) -> Response {
    println!("a: in");
    let res = next.run(req).await;
    println!("a: out");
    res
}

async fn b(req: Request, next: Next) -> Response {
    println!("b: in");
    let res = next.run(req).await;
    println!("b: out");
    res
}

let app = Router::new()
    .route("/", get(hello))
    .layer(middleware::from_fn(a))
    .layer(middleware::from_fn(b));
```

A request to `/` prints:

```text
b: in
a: in
a: out
b: out
```

Reading a long chain of `.layer()` calls from top to bottom therefore gives you the order from the handler outward, which is the opposite of how most people read it.

### `ServiceBuilder` reads top to bottom

`tower::ServiceBuilder` fixes this. Layers added to a `ServiceBuilder` run in the order you write them, and you apply the whole stack with a single `.layer()`:

```rust
use tower::ServiceBuilder;

let app = Router::new()
    .route("/", get(hello))
    .layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http()) // runs first
            .layer(CorsLayer::permissive())
            .layer(middleware::from_fn(require_token)), // runs last, closest to handler
    );
```

For anything beyond two or three layers, this is the form worth using. A good default order is:

1. Tracing, so every request is logged, including rejected ones.
2. Timeouts and compression.
3. CORS, so preflight requests are answered before authentication rejects them.
4. Authentication and other checks that depend on the request itself.

## Which routes does a layer cover?

Two details trip people up.

First, `Router::layer` wraps every route that is **already registered**. Routes added afterwards are not covered:

```rust
let app = Router::new()
    .route("/public", get(hello))
    .layer(middleware::from_fn(timing))   // covers /public
    .route("/private", get(whoami));      // not covered by timing
```

Second, `Router::layer` also wraps the router's fallback, which is what produces `404` responses. If your authentication layer is applied this way, an unauthenticated request to a path that does not exist gets a `401` instead of a `404`. Use `route_layer` when you only want the middleware to run for requests that matched a route:

```rust
let protected = Router::new()
    .route("/me", get(whoami))
    .route_layer(middleware::from_fn(require_token));

let app = Router::new()
    .route("/", get(hello))
    .merge(protected);
```

Splitting the app into a public router and a protected router, then merging them, is a clean way to apply authentication to only part of the API.

## Testing middleware

Middleware is part of the `Router`, so the in-process testing approach from the previous post works unchanged. Here we check that the auth layer rejects a request with no token:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest};
    use tower::ServiceExt;

    #[tokio::test]
    async fn rejects_missing_token() {
        let app = Router::new()
            .route("/me", get(whoami))
            .route_layer(middleware::from_fn(require_token));

        let response = app
            .oneshot(HttpRequest::builder().uri("/me").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
```

Because `require_token` short-circuits, `whoami` never runs and its `Extension` extractor is never evaluated. If it had run, you would see the `500` described earlier.

## Wrapping up

- Middleware in Axum is `tower`: a `Layer` wraps a `Service`.
- `from_fn` is the quickest way to write your own, and `from_fn_with_state` gives it access to shared state.
- Extensions are how middleware passes data to handlers.
- `tower-http` supplies tracing, CORS, compression and timeouts.
- Each `.layer()` call wraps what came before it, so later layers run first. Use `ServiceBuilder` when you want the code to read in execution order.
- Use `route_layer` when a layer should skip unmatched paths.
