# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo run                            # Run dev server (default: http://127.0.0.1:8084)
RUST_LOG=debug cargo run             # Run with debug logging
cargo run -- hash-password <pass>    # Generate bcrypt hash for admin password
mysql -u root -p < sql/init.sql      # Initialize database (creates DB + tables)
```

No test suite exists yet. Use `cargo build` to verify changes compile.

## Architecture

Actix-web 4 server with Tera templates, MySQL via SQLx, session-based auth.

### Request Flow

```
Request → SessionMiddleware → Logger → AuthGuard → Route Handler → Tera Template → Response
```

**AuthGuard** (`src/middleware.rs`) checks session for all routes except `/login`, `/static/*`, `/extract/*`. Unauthenticated requests redirect to login.

### Key Modules

- **`src/main.rs`** — Server bootstrap: loads config, initializes DB pool + Tera, configures middleware chain and routes
- **`src/routes/qrcode.rs`** — All QR code business logic (~440 lines): CRUD, image generation, extraction with atomic counter, extraction log viewer. This is the main file for feature work.
- **`src/routes/auth.rs`** — Login (bcrypt verify), logout (session purge)
- **`src/middleware.rs`** — AuthGuard: custom actix Transform/Service impl checking session cookies
- **`src/config.rs`** — Deserializes `config.toml` into typed structs (server, admin, database sections)

### QR Code Extraction Security

Extraction URLs contain an HMAC-SHA256 hash (first 8 hex chars) computed from `uuid + extract_salt`. The `verify_extract_hash()` check in `extract_page`/`extract_handler` prevents URL tampering. The atomic UPDATE (`WHERE used_count < max_count`) prevents concurrent over-extraction.

### Database

Two tables in MySQL (`sql/init.sql`):
- **`qr_codes`** — uuid (unique), text_content, remark, max_count, used_count, last_extract_ip, last_extract_at, created_at
- **`qr_extract_logs`** — qrcode_id (indexed), client_ip, extracted_at

The app starts without a DB connection (warns and continues), but all features require it.

### Templates

Tera templates in `templates/` extend `base.html`. Admin pages use `body-admin`/`page-admin` layout classes. Public extract pages use `body-extract`/`page-center`. Pagination is JS-driven (inline `<script>` blocks reading `data-page`/`data-total` attributes).

### Configuration

`config.toml` (see `config.example.toml`):
- `server.context_path` — Virtual directory prefix (e.g., `/qrcode`), affects all routes and static assets
- `server.public_host` — Used in QR code image URLs; must match the externally reachable address
- `server.extract_salt` — HMAC key for extraction URL signing

### Logging Convention

Uses `log` crate with `env_logger`. Levels: **debug** for request params and flow tracing, **info** for business events (login, create, extract), **warn** for failures (auth, exhausted codes, DB errors). Control via `RUST_LOG` env var.
