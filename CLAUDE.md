# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo run                            # Run dev server (default: http://127.0.0.1:8084)
RUST_LOG=debug cargo run             # Run with debug logging
cargo run -- hash-password <pass>    # Generate bcrypt hash for admin password
cargo test                           # Run 30 unit tests (helpers, rate_limit, csrf)
mysql -u root -p < sql/init.sql      # Initialize database (creates DB + tables)
```

Cross-compile for Linux: `cargo build --release --target x86_64-unknown-linux-musl`

## Architecture

Actix-web 4 server with Tera templates, MySQL via SQLx, session-based auth.

### Request Flow

```
Request → SessionMiddleware → Logger → AuthGuard → Route Handler → Tera Template → Response
```

**AuthGuard** (`src/middleware.rs`) checks session for all routes except `/login`, `/static/*`, `/extract/*`. Unauthenticated requests redirect to login.

### Key Modules

- **`src/main.rs`** — Server bootstrap: loads config, initializes DB pool + Tera, configures middleware chain and routes
- **`src/models.rs`** — Data structures: QrCodeRecord, ExtractLog, form/query structs (CreateForm, ActionForm, ListQuery, LogsQuery)
- **`src/helpers.rs`** — Constants (PAGE_SIZE, MAX_CONTENT_LENGTH, MAX_COUNT_UPPER), HMAC hash, segment parsing, template rendering, `db_try!`/`db_try_optional!` macros, `validate_segments()`, pagination helpers
- **`src/csrf.rs`** — Per-session CSRF token generation and validation
- **`src/rate_limit.rs`** — IP-based login rate limiter with periodic expired-entry cleanup
- **`src/routes/admin.rs`** — Admin CRUD handlers: list, create, edit, delete, reset, logs, download_image
- **`src/routes/extract.rs`** — Public extract handlers: extract_page (GET), extract_claim_handler (POST JSON)
- **`src/routes/auth.rs`** — Login (bcrypt verify + rate limit + CSRF), logout (session purge)
- **`src/middleware.rs`** — AuthGuard: custom actix Transform/Service impl checking session cookies
- **`src/config.rs`** — Deserializes `config.toml` into typed structs; validates secret_key ≥ 64 chars

### Security

- **CSRF**: Per-session token validated on all admin POST routes; `SameSite=Strict` cookies as second layer
- **Session cookies**: HttpOnly, Secure (HTTPS mode), SameSite=Strict, 8-hour TTL
- **Login rate limiting**: 10 attempts per 5-minute window per IP
- **HMAC**: 16 hex chars (64-bit), constant-time comparison via `subtle` crate; legacy 8-char hash backward compatible (`legacy_hash_support` config)
- **Config validation**: secret_key must be ≥ 64 characters; DB connection required at startup

### QR Code Extraction (browser_id / slot model)

Extraction URLs contain an HMAC-SHA256 hash (16 hex chars) computed from `uuid + extract_salt`. Each `browser_id` (UUID v4 generated client-side, stored in localStorage) claims one segment sequentially via POST `/claim` (JSON API). The claim uses `SELECT ... FOR UPDATE` row lock + transaction to prevent concurrent over-allocation. Idempotent: same browser_id returns cached segment.

### Database

Three tables in MySQL (`sql/init.sql`):
- **`qr_codes`** — uuid (unique), text_content (JSON array of segments), remark (indexed), max_count, used_count, last_extract_ip, last_extract_at, created_at
- **`qr_extract_logs`** — qrcode_id (indexed, FK cascade), client_ip, browser_id, segment_index, extracted_at
- **`qr_browser_slots`** — qrcode_id + browser_id (unique, FK cascade), segment_index, client_ip, assigned_at

DB connection is required at startup — app exits if connection fails. Connection pool size and timezone are configurable.

### Templates

Tera templates in `templates/` extend `base.html`. Admin pages use `body-admin`/`page-admin` layout classes. Public extract pages use `body-extract`/`page-center`. All POST forms include a hidden `csrf_token` field. List page delete/reset buttons inject CSRF token via `data-csrf` attribute on `<main>`. Pagination is JS-driven (inline `<script>` blocks reading `data-page`/`data-total` attributes).

### Configuration

`config.toml` (see `config.example.toml`):
- `server.secret_key` — Session signing key, must be ≥ 64 characters
- `server.context_path` — Virtual directory prefix (e.g., `/qrcode`), affects all routes and static assets
- `server.public_host` — Used in QR code image URLs; must match the externally reachable address
- `server.extract_salt` — HMAC key for extraction URL signing
- `server.legacy_hash_support` — Accept old 8-char HMAC hashes (default: true)
- `database.max_connections` — Connection pool size (default: 10)
- `database.timezone` — Session timezone for MySQL (default: `+08:00`)

### Logging Convention

Uses `log` crate with `env_logger`. Levels: **debug** for request params and flow tracing, **info** for business events (login, create, extract), **warn** for failures (auth, exhausted codes, DB errors). Control via `RUST_LOG` env var.

### Deployment Notes

- Cross-compile with musl target for static linking: `cargo build --release --target x86_64-unknown-linux-musl`
- When syncing templates/static to server, use `scp files/*.ext host:dest/` — do NOT use `scp -r dir/ host:dest/` as it creates nested subdirectories instead of overwriting
- After updating templates or static files, restart the service to reload Tera templates
