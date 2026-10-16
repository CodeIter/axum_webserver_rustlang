# axum-server

Modular, production-oriented web server on **Axum 0.8**.

```
cargo run --release -- --port 3000 --log-format json
```

Config precedence: **CLI flags > env vars / `.env` > defaults in `src/config.rs`**.
Run `cargo run -- --help` for all options; see `.env.example` for env names.

| Endpoint (`/api/v1`)                       | Description                          |
|--------------------------------------------|--------------------------------------|
| `GET /hello`                               | "Hello World"                        |
| `GET /ready`                               | Readiness (503 while shutting down)  |
| `GET /health`                              | Status + system metrics              |
| `GET /number/{min}/{max}/{quantity}`       | Random integers                      |
| `GET /password/{length}/{quantity}?chars=` | CSPRNG passwords                     |
| `GET /redirect/{proto}/{domain}/{*path}?…` | 307 redirect                         |

Everything else is served from `--static-root` (default `./public`).
Access log -> stdout; all other logs -> stderr; both support `--log-format text|json`.

## Running on Render

* Deploy with the included Alpine `Dockerfile`; set the health check path to `/api/v1/health`.
* Render injects `PORT`, which the server reads directly.
* Client IP precedence: `CF-Connecting-IP` > `True-Client-IP` > `X-Forwarded-For` > socket.
  `X-Forwarded-For` is counted **from the right** (`XFF_PROXY_DEPTH`, default 3 =
  client, Cloudflare edge, Render LB); the spoofable leftmost entry is never used.
  If the chain is shorter than the depth, the socket address is used.
* `GET /api/v1/ip` returns `{"ip": "...", "source": "cf-connecting-ip"}`.
* Set `TRUST_PROXY_HEADERS=false` if the service is ever reachable without these proxies.
