# axum-server

A production-grade, modular web server built on Rust and Axum 0.8. It serves server-rendered Handlebars pages, a Markdown-powered blog with YAML front matter, a JSON API under `/api/v1`, and static files, all behind a hardened middleware stack that handles client IP resolution, rate limiting, concurrency limiting, request timeouts, security headers, and structured access logging. Configuration is layered (command-line flags, then environment variables or a `.env` file, then built-in defaults), and the server shuts down gracefully with a hard deadline. A multi-stage Dockerfile produces a small static musl binary on Alpine, ready to deploy.

## Features

- Server-rendered pages with Handlebars templates, layouts, and partials
- Markdown blog with YAML front matter, series support, sanitized HTML output, and "load more" pagination
- Automatically generated `sitemap.xml` and `robots.txt`
- JSON API under `/api/v1` with uniform error responses
  - Health, readiness, and client IP endpoints
  - In-memory key-value store with full CRUD and a configurable entry cap
  - Random number and password generators
  - Request header and forwarding header inspection
  - URL redirect endpoint
- Static file serving from `public/`, `static/`, and `resources/`, with a configurable index file and dotfile policy (`allow`, `deny`, `ignore`)
- Custom 404 page rendered from a template
- Per-client fixed-window rate limiting, with IPv6 clients grouped by `/64` prefix
- Load-shedding concurrency limiter that returns `503` instead of queueing
- Request timeout returning `408`
- Security headers on every response: CSP, HSTS, `X-Frame-Options`, `X-Content-Type-Options`, and more
- Trusted client IP detection from `CF-Connecting-IP`, `True-Client-IP`, and `X-Forwarded-For`
- Structured access log on stdout and diagnostics on stderr, in text or JSON format
- Graceful shutdown with a configurable timeout
- Layered configuration: CLI flags override environment variables, which override defaults

## Getting Started

### Prerequisites

- A recent stable [Rust toolchain](https://rustup.rs/)

### Run in development

```bash
git clone https://github.com/CodeIter/axum_webserver_rustlang axum-server
cd axum-server
cp .env.example .env   # optional create .env file
vim .env               # optional edit .env file
cargo run
```

The server listens on `http://0.0.0.0:8000` by default. Run it from the project root so it can find the `templates/`, `blog/`, `public/`, `static/` and `resources/` directories.

### Build for release

```bash
cargo build --release
./target/release/axum-server
```

The release profile enables thin LTO, a single codegen unit, and symbol stripping for a small, fast binary.

### Command-line options

```bash
cargo run -- --help
cargo run -- --port 8000
```

### Configuration

Every option can be set as a CLI flag, an environment variable, or a line in `.env`. See `.env.example` for a starting point.
See `--help` CLI flag for details.

### Endpoints

| Path | Description |
|------|-------------|
| `/`, `/about`, `/hello/world` | Rendered pages |
| `/blog`, `/blog/{date}/{slug}` | Blog index and posts |
| `/sitemap.xml`, `/robots.txt` | SEO files |
| `/api/v1/hello` | Plain text greeting |
| `/api/v1/health` | Status and system metrics |
| `/api/v1/ready` | Readiness probe (`503` during shutdown) |
| `/api/v1/ip` | Detected client IP and its source |
| `/api/v1/blog/posts?offset=&limit=` | Paginated posts with rendered HTML cards |
| `/api/v1/kv/memory/items` | Key-value store (`GET`, `POST`) |
| `/api/v1/kv/memory/items/{key}` | Key-value store (`GET`, `PUT`, `DELETE`) |
| `/api/v1/number/{min}/{max}/{quantity}` | Random numbers |
| `/api/v1/password/{length}/{quantity}` | Random passwords, optional `?chars=` |
| `/api/v1/http/headers`, `/api/v1/http/xff` | Request header inspection |
| `/api/v1/redirect/{proto}/{domain}/{*path}` | `307` redirect to the target URL |

### Writing blog posts

Add a Markdown file with YAML front matter to the `blog/` directory and restart the server. Posts are listed newest first.

## Deploying to Render

The recommended way to deploy is [Render](https://render.com) with the included `Dockerfile`. The image builds a static musl binary, runs as a non-root user, bundles the templates, blog, and static directories, and includes a health check against `/api/v1/health`.

1. Push the repository to GitHub, GitLab, or Bitbucket.
2. In the Render dashboard, choose **New** and then **Web Service**, and connect the repository.
3. Set **Language** to **Docker**. Render detects the `Dockerfile` at the repository root automatically.
4. Set the **Health Check Path** to `/api/v1/health`.
5. Add the following environment variables:

   | Variable | Value | Why |
   |----------|-------|-----|
   | `PORT` | `8000` | Matches the port the container listens on. **Not required**, the is port is hard coded in Dockerfile to 8000 and exposed, so render can detect it automatically. |
   | `TRUST_PROXY_HEADERS` | `true` | Render sits behind a proxy, so client IPs come from forwarded headers |
   | `XFF_PROXY_DEPTH` | `3` | Client, Cloudflare edge, Render load balancer |
   | `LOG_FORMAT` | `json` | Structured logs for Render's log viewer |

Note: Render keeps the HTTP headers `CF-Connecting-IP` and `True-Client-IP` from Cloudflare, so the IP will be detected automatically if you set `TRUST_PROXY_HEADERS` to `true`. Therefore, setting `XFF_PROXY_DEPTH` is optional in this context.

6. Click **Create Web Service**. Render builds the image and deploys it.

`RENDER_EXTERNAL_URL` is set by Render automatically, so `sitemap.xml` and `robots.txt` use your service URL without extra configuration. To use a custom domain, set `PUBLIC_URL` to it.

Notes:

- The key-value store lives in memory, so its contents are lost on every restart or redeploy.
- Blog posts, templates, and static files are baked into the image, so publishing a new post means redeploying.

### Test the image locally

```bash
docker build -t axum-server .
docker run --rm -p 8000:8000 axum-server
```

## Project Structure

```
src/
  main.rs         Startup and wiring
  app.rs          Router and middleware stack assembly
  config.rs       CLI, environment, and default configuration
  logging.rs      Access log (stdout) and diagnostics (stderr)
  error.rs        Error types and JSON error responses
  shutdown.rs     Signal handling and graceful shutdown
  routes/         Route tables
  handlers/       Thin HTTP adapters
  middleware/     Rate limit, concurrency, timeout, headers, logging, static guard
  services/       Business logic independent of HTTP
templates/        Handlebars layouts, partials, and pages
blog/             Markdown posts with YAML front matter
public/           Static files served at /
static/           Static files served at /static
resources/        Files served at /resources
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---
Coded with ❤️ by [@CodeIter](https://github.com/CodeIter)
