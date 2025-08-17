# Media Server — Architecture

## Overview
Small Rust media server that scans a configured directory, indexes metadata into SQLite, and exposes HTTP APIs to trigger scans, list directory contents, and fetch item details. HTTP streaming is planned but not implemented yet in this codebase.

## Goals
- Idempotent indexing (repeated scans do not create duplicates).
- Simple, predictable data model with fast lookups by unique path.
- Clear separation of concerns between scanning, persistence, and HTTP handlers.

## Tech stack
- axum 0.6 for HTTP routing/handlers
- sqlx 0.7 with SQLite for persistence (pooled SqlitePool)
- tokio for async runtime and filesystem operations (tokio::fs)
- mime_guess for MIME detection
- config for config management; clap for the CLI

## High-level components (server/src)
- `main.rs`: bootstrap, configuration loading, DB init, and route wiring.
- `config.rs`: typed configuration (`AppConfig`).
- `state.rs`: shared runtime state (`AppState` with `SqlitePool` and media root path) wrapped in `Arc<Mutex<...>>` for handlers.
- `db.rs`: schema initialization and repository helpers (upsert, list, get by id/path) using sqlx.
- `scanner.rs`: async filesystem traversal (DFS) with batching and sqlx transactions.
- `handlers.rs`: axum handlers that call scanner/db and return JSON responses.
- `models.rs`: domain structs (`MediaEntry`, `NewMediaEntry`).

## Database schema
Table: `media`
- `id` INTEGER PRIMARY KEY
- `name` TEXT NOT NULL
- `path` TEXT NOT NULL UNIQUE    — stored relative to the configured media root
- `parent_id` INTEGER            — nullable FK to `media.id`
- `mime_type` TEXT               — null for directories
- `size` INTEGER                 — null for directories
- `tags` TEXT                    — optional JSON-encoded array of strings
- `created_at` DATETIME DEFAULT CURRENT_TIMESTAMP

Indexes
- `idx_parent_id` on (`parent_id`)
- `idx_path` on (`path`) — uniqueness + index keeps lookups/idempotency fast

Notes
- "Directory vs file" is inferred: directories have `mime_type = NULL` and `size = NULL`.
- Path uniqueness enables `INSERT ... ON CONFLICT(path) DO UPDATE` upserts.

## Scanning and indexing
- Depth-first traversal starting from the configured root, using `tokio::fs::read_dir` and a stack.
- Paths are stored relative to the root (validated/constructed during scan).
- Directories are upserted immediately to obtain their `id` for children.
- Files are buffered and written in batches inside a sqlx transaction; default batch size is 500.
- Upsert semantics centralize in `db::upsert_media{,_in_tx}` and use `ON CONFLICT(path) DO UPDATE`.
- MIME type is guessed with `mime_guess`; file size comes from metadata.

## HTTP API
Framework: axum. Handlers use repository functions; errors map to appropriate HTTP statuses.

Endpoints
- POST `/scan`
  - Triggers a directory scan. Returns 200 on success; 500 on error.
  - The handler clones pool/root from `AppState`, drops the mutex, then runs the scan.

- GET `/media`
  - Query: `parent_id` (optional) or `path` (relative to root). Optional `tags` CSV to filter items containing all provided tags.
  - If `path` resolves to a directory, returns its children. If it resolves to a file, returns a single-item `files` array.
  - Response: `{ "files": [MediaEntry, ...] }`.

- GET `/media/details`
  - Query: `path` parameter. If the value is numeric, it is treated as an `id`; otherwise it is treated as a relative `path`.
  - Response: a `MediaEntry` JSON object on success; 404 if not found; 400 for invalid path (must be relative, no `..`).

- GET `/media/stream`
  - Not implemented yet in this codebase. See Future enhancements for intended behavior.

## Models
`MediaEntry`
- `id: i64`
- `name: String`
- `path: String` (relative to root)
- `parent_id: Option<i64>`
- `mime_type: Option<String>`
- `size: Option<i64>`
- `created_at: String`
- `tags: Option<Vec<String>>`

`NewMediaEntry` mirrors the above minus `id/created_at`.

## Configuration
Loaded via `config` crate into `AppConfig` with keys:
- `db_path`: SQLite file path (e.g., `media.db`)
- `directory_to_scan`: root directory to index (absolute or relative)
- `host` (optional): default `127.0.0.1`
- `port` (optional): default `8080`

## CLI
`scan` subcommand runs a one-off scan and exits, e.g.: `cargo run --manifest-path ./server/Cargo.toml -- scan`.

## Runtime and concurrency notes
- Database access uses `sqlx::SqlitePool` (pooled connections) shared via `AppState`.
- Handlers take `Arc<Mutex<AppState>>`; scanning clones the pool and path, releases the mutex, then works asynchronously.
- File writes are performed inside sqlx transactions for batch durability; filesystem operations use `tokio::fs` to avoid blocking the runtime.

## Security and sanitization
- Handlers validate requested paths are relative and do not contain `..` to avoid directory traversal.
- Prefer lookups by `id` where possible; when accepting paths from clients, always treat them as relative to the configured root.

## Testing
- Recommended: unit tests for repository helpers against a temp/in-memory SQLite, plus integration tests that create a temp dir, run a scan, and exercise the HTTP endpoints.

## Future enhancements
- Implement `/media/stream` with HTTP Range support (`Accept-Ranges`, `Content-Range`, partial content 206) using async file IO and chunked bodies.
- Pagination for `/media`.
- Authentication and ACLs.
- Optional transcoding (HLS/DASH) and caching headers (ETag/Last-Modified).
- Schema migrations for upgrades.

## Operational tips
- For production-scale streaming, front with a static server (e.g., nginx) or CDN with signed URLs.
- Vacuum/back up the SQLite database periodically.
