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
| `GET /ip`                                  | Detected client IP + source          |
| `GET /http/headers`                        | All request headers (JSON)           |
| `GET /http/xff`                            | Raw CF-Connecting-IP / True-Client-IP / X-Forwarded-For |
| `GET /number/{min}/{max}/{quantity}`       | Random integers                      |
| `GET /password/{length}/{quantity}?chars=` | CSPRNG passwords                     |
| `GET /redirect/{proto}/{domain}/{*path}?…` | 307 redirect                         |

### Pages, templates and request order

Handlebars templates live in `--templates-dir` (`TEMPLATES_DIR`, default `./templates`):
`layouts/layout.hbs`, `partials/{meta,navigation}.hbs`, `index.hbs` (`/`), `about.hbs`
(`/about`), `hello_world.hbs` (`/hello/world`, wrapped in its own `layouts/layout_hello.hbs`) and `404.hbs`. All of these are required; the server exits at startup if one is missing.

Requests flow through the global middleware, then: **template routes -> `/api/v1` API ->
static files (`--static-root`, default `./public`) -> custom 404 page**.
Unknown `/api/v1/*` paths return a JSON 404 instead of the HTML page.
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

## Markdown blog

Posts are `*.md` files in `--blog-dir` (`BLOG_DIR`, default `./blog`), loaded once at
startup (restart to pick up changes). Pages: `/` (5 latest), `/blog` (10 latest),
`/blog/{createdAt date}/{slug}`; "Load more" uses `GET /api/v1/blog/posts?offset=&limit=`
(JSON with `posts` plus server-rendered `html` cards; `limit` is capped at 50).

All front-matter fields are optional and parsed leniently (a bad value logs a warning and
falls back to its default):

| Field | Default |
|-------|---------|
| `slug` | file name without extension (URL-safe, lower-cased) |
| `title` | derived from the slug (`my-post` -> `My Post`) |
| `excerpt` | first paragraph, capped at 160 characters |
| `author` (string / list) | `Anonymous` |
| `license` | `CC BY-SA` |
| `createdAt` / `updatedAt` (ISO date or datetime; naive values are UTC) | file creation / modification time |
| `keywords` (list or comma-separated string) | empty |
| `category`, `serie`, `image`, `imageAlt`, `canonicalUrl` | empty |
| `lang` | `en` |
| `published` / `featured` | `true` / `false` |
| `readingTime` | always computed (200 words/min, minimum 1); front-matter value ignored |

Notes: unpublished posts are hidden everywhere, including their direct URL. `image` and
`canonicalUrl` accept `http(s)` URLs or site paths only. Markdown HTML output is sanitized
(no scripts, event handlers or `javascript:` URLs). Reference images with absolute paths
(`/img/x.png`, served from `public/`). Filesystem dates are unreliable in Docker images
(they reflect the build), so set `createdAt` explicitly for real posts.

## UI

Templates use [W3.CSS 5](https://www.w3schools.com/w3css/5/w3.css) loaded from w3schools.com
(allowed in the CSP's `style-src`; remote `https:` images are allowed in `img-src`).
Because inline scripts/styles are blocked by the CSP, behaviour lives in `public/js/` and
custom CSS in `public/css/style.css`.
