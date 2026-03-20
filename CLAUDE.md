# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo run                            # Run dev server (default: http://127.0.0.1:8084)
RUST_LOG=debug cargo run             # Run with debug logging
cargo run -- hash-password <pass>    # Generate bcrypt hash for admin password
cargo test                           # Run 34 unit tests
mysql -u root -p < sql/init.sql      # Initialize database (new installs only)
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

- **`src/main.rs`** — Server bootstrap: loads config, initializes DB pool + Tera, configures middleware chain and routes; includes `JsonConfig::limit(4096)` and `FormConfig::limit(65536)`
- **`src/models/domain.rs`** — Data structures: QrCodeRecord, ExtractLog, AuditLog, AdminUser
- **`src/models/request.rs`** — Form/query structs: CreateForm, ActionForm, ListQuery, LogsQuery, AuditLogsQuery, ClaimRequest, CreateUserForm, ToggleUserForm, ChangePasswordForm
- **`src/utils/`** — crypto (HMAC), pagination, render (template helpers, `db_try!` macros), validation (`get_client_ip`, segment parsing, constants)
- **`src/services/`** — qrcode (CRUD + image generation), extract (slot claim), audit (operation logging), ai (comment generation), user (multi-admin CRUD + login verify)
- **`src/csrf.rs`** — Per-session CSRF token generation and validation
- **`src/rate_limit.rs`** — IP-based login rate limiter with periodic expired-entry cleanup
- **`src/routes/admin.rs`** — Admin CRUD handlers: list, create, edit, delete, reset, logs, download_image, audit_logs_page, AI generate, users_page, create_user, toggle_user, change_password (role-based access)
- **`src/routes/extract.rs`** — Public extract handlers: extract_page (GET), extract_claim_handler (POST JSON)
- **`src/routes/auth.rs`** — Login (config super admin → DB admin fallback, bcrypt verify + rate limit + CSRF), logout (session purge). Session stores `"user"` + `"role"` ("super"/"admin")
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

Five tables in MySQL (`sql/init.sql` + `sql/migrations/`):
- **`qr_codes`** — uuid (unique), text_content (JSON array of segments), remark (indexed), max_count, used_count, last_extract_ip, last_extract_at, created_at
- **`qr_extract_logs`** — qrcode_id (indexed, FK cascade), client_ip, browser_id, segment_index, extracted_at
- **`qr_browser_slots`** — qrcode_id + browser_id (unique, FK cascade), segment_index, client_ip, assigned_at
- **`admin_audit_logs`** — username, action, target_uuid, detail, client_ip, created_at (indexes on created_at, username, action)
- **`admin_users`** — username (unique), password_hash, is_active, locked_until, failed_attempts, created_at, updated_at

DB connection is required at startup — app exits if connection fails. Connection pool size and timezone are configurable.

### Database Migrations

Project is live — **never modify `sql/init.sql`**. All schema changes use incremental migration files:
- `sql/migrations/NNN_description.sql` (e.g., `001_add_audit_logs.sql`)
- Apply manually: `mysql -u root -p qrcode < sql/migrations/001_add_audit_logs.sql`

### Templates

Tera templates in `templates/` extend `base.html`. Admin pages use sidebar layout (`nav.html` included via `{% include %}`). Each admin page passes `active_nav` ("qrcode"/"audit"/"ai"/"users"/"password"), `ai_enabled`, and `role` ("super"/"admin") to control navigation visibility. Mobile uses hamburger menu + sliding sidebar overlay. Public extract pages use `body-extract`/`page-center`. All POST forms include a hidden `csrf_token` field. List page delete/reset buttons inject CSRF token via `data-csrf` attribute on `<main>`. Pagination is JS-driven (inline `<script>` blocks reading `data-page`/`data-total` attributes).

### Multi-Admin Roles

- **Super admin** (config file `[admin]`): full access including audit logs and user management
- **Regular admin** (DB `admin_users` table): QR management, AI generate, change own password
- AuthGuard unchanged (checks `"user"` session); role-based access enforced in handlers via `is_super_admin()`
- Account locking: 5 failed login attempts → 30-minute lock on DB users

### QR Code Image Generation

`generate_qr_image()` in `src/services/qrcode.rs` renders styled QR with blue→purple gradient, module gaps, white padding. Remark text drawn below using system CJK font loaded at runtime (macOS: PingFang/STHeiti; Linux: Noto Sans CJK). Emoji characters filtered before rendering. No embedded font files — binary stays small.

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

Uses `log` crate with `env_logger`. **Do not log sensitive data** (IP, username, UUID, browser_id) — CodeQL taint tracking will flag it. Use `services::audit::log_action()` for operation tracking instead. Levels: **debug** for flow tracing, **info** for business events, **warn** for failures. Control via `RUST_LOG` env var.

### CI/CD

- GitHub Actions (`.github/workflows/rust.yml`): builds with `x86_64-unknown-linux-musl` for static linking
- Tag push (`v*`) triggers automatic Release with artifact
- CodeQL security scanning runs automatically on push to main
- `gh` CLI used for releases, issues, and project management

### Deployment Notes

- Cross-compile with musl target for static linking: `cargo build --release --target x86_64-unknown-linux-musl`
- When syncing templates/static to server, use `scp files/*.ext host:dest/` — do NOT use `scp -r dir/ host:dest/` as it creates nested subdirectories instead of overwriting
- After updating templates or static files, restart the service to reload Tera templates

### Environment

- Local dev MySQL runs in k8s at `127.0.0.1:30306` (see `config.toml` `[database].url`)
- Do NOT install/start local MySQL via brew — use the k8s instance
