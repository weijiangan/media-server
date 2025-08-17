# Media Server (server)

This document explains how to run the server locally, the available APIs, and notes about configuration and development.

Prerequisites
- Rust (stable toolchain)
- SQLite

Build & run

From the repository root:

```bash
# build
cargo build --manifest-path ./server/Cargo.toml

# run (reads config from server/config.json)
cargo run --manifest-path ./server/Cargo.toml
```

Run a one-off scan from CLI:

```bash
cargo run --manifest-path ./server/Cargo.toml -- scan
```

Configuration

Edit `server/config.json` (or use environment variables supported by `config` crate). Example:

```json
{
  "db_path": "media.db",
  "directory_to_scan": "./media",
  "host": "127.0.0.1",
  "port": 8080
}
```

APIs

- POST /scan
  - Trigger a directory scan (updates database). Returns 200 on success.

- GET /media?parent_id={id}
  - List child entries of `parent_id`. Use `parent_id` omitted for root.
  - Response: { "files": [ { id, name, path, type, size }, ... ] }

- GET /media/details?id={id} or GET /media/details?path={path}
  - Get a single entry by id or path.

- GET /media/stream?id={id} or GET /media/stream?path={path}
  - Streams the file. Supports HTTP `Range` header for seeking.

Streaming examples

Download entire file:

```bash
curl "http://127.0.0.1:8080/media/stream?path=/full/path/to/file.mp4" -o file.mp4
```

Request a range (seek):

```bash
curl -H "Range: bytes=100000-" "http://127.0.0.1:8080/media/stream?path=/full/path/to/file.mp4" -o partial.bin
```

Development notes

- DB access is implemented in `server/src/db.rs`.
- Scanner uses `server/src/scanner.rs` and calls DB helpers for upsert.
- HTTP handlers are in `server/src/handlers.rs`.
- Add tests under `server/tests` for integration testing.

