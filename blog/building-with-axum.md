---
title: Building a web server with Axum
slug: building-with-axum
excerpt: A tour of the router, extractors and shared state in an Axum 0.8 server.
author: [Ada Lovelace, Grace Hopper]
license: CC BY 4.0
createdAt: 2026-09-12
updatedAt: 2026-10-01T09:30:00Z
keywords: rust, axum, web
category: Rust
serie: Axum Basics
lang: en
featured: true
---
Axum is a web framework built on `tokio`, `tower` and `hyper`.

```rust
async fn hello() -> &'static str {
    "Hello World"
}
```

> Handlers are plain async functions.

| Concept   | Purpose                  |
|-----------|--------------------------|
| Router    | Maps paths to handlers   |
| Extractor | Parses request data      |
