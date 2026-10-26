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

## Sitemap, robots.txt and public hostname

`GET /sitemap.xml` is generated per request from: the template pages (`PAGE_PATHS` in
`src/routes/pages.rs`; add new pages there too), every published blog post (with `lastmod` =
`updatedAt`), and public `.html`/`.htm` files in the static directory (hidden files/directories
are never listed; `dir/index.html` appears as `/dir/`; the root `index.html` is skipped because
`/` is a template route). `GET /robots.txt` allows crawling, disallows `/api/`, and links the sitemap.
Both routes take precedence over files of the same name in `public/`.

Absolute URLs use the public hostname, resolved in this order (first valid value wins):
`--public-url` > `RENDER_EXTERNAL_URL` > `RENDER_EXTERNAL_HOSTNAME` > `EXTERNAL_URL` > `EXTERNAL_HOSTNAME`.
Bare hostnames get `https` (`http` for localhost). If nothing is configured the request `Host`
header is used (validated, but client-controlled), and a warning is logged at startup.

## CI/CD (`.github/workflows/`)

| Workflow | Trigger | Does |
|----------|---------|------|
| `ci.yaml` | push to `main` | `cargo fmt --check`, `test`, `clippy -D warnings`, `build --release`, then publishes a **pre-release** `pre-<run>-<sha>` with the Linux binary |
| `pullrequest.yaml` | PR opened / updated / reopened | same checks, `cargo build` |
| `release.yaml` | tag `v*` | same checks, then a formal **GitHub Release** with the Linux binary |

Release assets: `axum-server-linux-x86_64` (bare binary), `axum-server-linux-x86_64.tar.gz`
(binary + `templates/`, `public/`, `blog/`, which the server needs at runtime) and `sha256sums.txt`.
The binary is built on `ubuntu-latest`, so it links against that runner's glibc.

## Key-value stores

Store implement the `KeyValueStore` trait (`src/services/store/`) and live in `AppState`:

* `state.memory`: in-memory `MemoryStore` (always on; bounded by `MEMORY_STORE_MAX_ENTRIES`; lost on restart).

Values are arbitrary JSON (max 64 KiB); keys are 1-256 characters. `create` fails with 409 if the key exists,
`update`/`delete` with 404 if it does not (no implicit upsert).

| Method & path (`/api/v1/kv/memory`) | Description |
|------|-------------|
| `GET /items?limit=&offset=` | list ordered by key (default 50, max 200) |
| `POST /items` `{"key","value"}` | create (201 / 409) |
| `GET /items/{key}` | read (200 / 404) |
| `PUT /items/{key}` `{"value"}` | update (200 / 404) |
| `DELETE /items/{key}` | delete (204 / 404) |

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

---
Coded with ❤️ by [@CodeIter](https://github.com/CodeIter)
