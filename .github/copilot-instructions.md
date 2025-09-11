- [x] Verify that the copilot-instructions.md file in the .github directory is created.

- [x] Clarify Project Requirements
  Project type: Monorepo with React TypeScript client and Rust server.

- [x] Scaffold the Project
  - React client files moved to 'client' directory.
  - Rust server files remain in 'server' directory.

- [x] Customize the Project
  - Verify that all previous steps have been completed successfully and you have marked the step as completed.
  - Develop a plan to modify codebase according to user requirements.
  - Ensure that only dependencies specified in package.json and Cargo.toml are used.
  - Apply modifications using appropriate tools and user-provided references.
  - Skip this step for "Hello World" projects.

- [x] Implement Server Functionality
	Implemented
	- HTTP server with Axum; endpoints: `POST /scan`, `GET /media` (supports `parent_id` or relative `path` + optional `tags`), `GET /media/details` (relative `path` or numeric id).
	- SQLite via `sqlx` with table `media` (id, name, unique relative path, optional `parent_id`, `mime_type`, `size`, optional `tags` JSON, timestamps). Indexes on `path` and `parent_id`.
	- Scanner uses `tokio::fs` for DFS traversal, upserts directories immediately (to obtain IDs), batches file upserts in sqlx transactions (batch size 500), and detects MIME via `mime_guess`.
	- Config loaded via `config` crate; runtime via `tokio`.
	Note: Replaced `rusqlite` with `sqlx`, and traversal is via `tokio::fs` (not `walkdir`).

- [x] Add Thumbnails and Streaming APIs
	Goal
	- Browse media with thumbnails; when viewing details, load full images or stream videos with HTTP Range.

	Plan
	1. Dependencies (Cargo.toml)
	   - Add `tokio-util` (for `ReaderStream`/`StreamBody`) for streaming. [DONE]
	   - Add `image` for image thumbnail generation (PNG/JPEG/WebP as needed). [DONE]
	   - Optional: integrate video thumbnailing via external `ffmpeg` CLI (document requirement) or add a Rust wrapper later. [PENDING]
      - Add `futures` (for buffer_unordered in admin regen). [DONE]
      - Add `tower-http` for static serving and CORS. [DONE]
      - Add `httpdate` and `sha2` for Last-Modified and ETag generation. [DONE]

	2. Storage/model changes
		- Store thumbnails on disk under a configurable thumbnails directory (defaults follow XDG or `/var/cache`) using deterministic names (e.g., `<media_id>_<wxh>.jpg`). [UPDATED - static serving added, generator writes to configured dir]
	   - Add nullable columns to `media` via lightweight migration: `width`, `height`, `duration_secs` (for videos), and `thumb_path` (relative path under `.thumbnails`). Keep data model backward compatible. [DONE]
	- Add nullable columns to `media` via lightweight migration: `width`, `height`, `duration_secs` (for videos), and `thumb_path` (stored as a server-visible URL path like `/thumbnails/<id>_<wxh>.jpg`). Keep data model backward compatible. [DONE]

	Recent updates (implemented)

	- `thumb_path` stores a server-visible URL (for example `/thumbnails/<id>_<wxh>.jpg`) and the server resolves that URL to files in the configured thumbnails directory. [DONE]
	- Image thumbnails use high-quality resizing (Lanczos3) and explicit JPEG quality. Thumbnail writes are atomic (tmp `.jpg` -> rename). [DONE]
	- Reusable helper `generate_thumbnail_for_entry` implemented; `generate_thumbnail_handler` delegates to it so admin + API share the same logic. [DONE]
	- Video poster extraction via `ffmpeg`/`ffprobe` is implemented as a best-effort path controlled by `ffmpeg_enabled`. On failure (or if disabled) the placeholder is returned. [DONE]
	- Thumbnails directory is configurable (`thumbnails_dir`) and statically served at `/thumbnails`. [DONE]
	- Placeholder thumbnail creation and a startup GC for stale temp-like thumbnail files are implemented. [DONE]
	- CORS is configurable via `cors_allowed_origins` and `cors_allow_credentials`. [DONE]
	- CORS is configurable via `cors_allowed_origins` and `cors_allow_credentials`. The server now uses a strict behaviour: CORS must be explicitly configured (or disabled via `cors_enabled = false`) and startup validation rejects `cors_allow_credentials = true` when origins are wildcard/Any. [DONE]
	- Resolved import/handler issues: aliased `axum::extract::Path` as `AxumPath` to avoid collision with `std::path::Path`, and updated the SPA handler to accept an optional path so `/` correctly serves `index.html`. [DONE]
	- SPA serving behavior: `build_client_service` implements SPA fallback (serves `index.html` when a requested file is missing) and is mounted only when `client_dist_dir` is explicitly configured. [DONE]
	- Dev config selection: prefer presence-based loading of `./config.dev.json` when present in the working directory (no environment variable required). [DONE]
  - ETag and Last-Modified headers:
    - Streaming: metadata-based ETag (SHA-256 over path+size+mtime) and Last-Modified headers added; honors `If-None-Match` and returns 304 when matched. [DONE]
    - Thumbnails: manual-serving path sets ETag and Last-Modified; static `/thumbnails` mount (ServeDir) handles conditional GETs. [DONE]

	3. Scanner updates
	   - On scan (or first request), generate thumbnails for images if missing or stale; record `thumb_path`, `width`, `height`. [PARTIAL - scanner records fields but automatic thumbnail generation during scan not implemented]
	   - For videos, generate a poster frame with `ffmpeg` (best-effort, skip if tool missing). [PENDING]

	4. APIs
	   - `GET /media/thumbnail?id|path[&w=..&h=..]`: returns a small image thumbnail (serve existing or generate on-demand; cache result; set appropriate `Content-Type` and caching headers). [IMPLEMENTED]
	     - Behavior: serves existing static thumbnail when present; when missing, `thumbnail_handler` now redirects clients to the generator endpoint.
		- `GET /media/generate_thumbnail?id|path&w&h`: on-demand generator that creates a thumbnail, writes it to the configured thumbnails directory (`<thumbnails_dir>/<id>_<wxh>.jpg`), upserts `thumb_path` (e.g., `/thumbnails/<id>_<wxh>.jpg`), and redirects to the static URL. [IMPLEMENTED]
	   - `GET /media/stream?id|path`: stream files (images, audio, video) with HTTP Range support. [IMPLEMENTED - basic single-range support]
	   - `GET /media/image?id|path` (alias): serve full-resolution images with correct `Content-Type` (alias to `stream`). [IMPLEMENTED]
	   - Responses include explicit `type` ("directory" | "file") and, for files, `kind` ("image" | "video" | "audio" | "other"). We currently do not embed `thumbnail_url`/`stream_url`; clients should call the relevant endpoints. [CURRENT]
  - Conditional GETs: ETag/Last-Modified headers added for streaming and thumbnail responses as above. [PARTIAL]

	5. Validation & security
	   - Enforce relative paths (no leading `/`, no `..`). Ensure resolved file paths stay under configured media root. [IMPLEMENTED]

	6. Testing
	   - Unit: DB model changes; thumbnail path resolver; Range parsing. [PENDING]
	   - Integration: create temp media dir with small image/video; run scan; assert `thumbnail` and `stream` endpoints; verify Range behavior and content types. [PENDING]

	7. Client (follow-up)
	   - Update React client to render grid with thumbnails and open viewer that uses `image_url` or `stream_url` from details. [PENDING]
	7. Client (follow-up)
	   - Update React client to render grid with thumbnails and open viewer that uses `image_url` or `stream_url` from details. [PENDING]
	   - Note: the React SPA in `client/` builds into `client/dist/`. Consider serving `client/dist` from the server (static ServeDir) in production or using a reverse-proxy. The server already supports mounting static directories via `tower-http::services::ServeDir` (see `/thumbnails` mounting) so adding a `/.` or `/app` static route is straightforward. [INFO]

	In progress / Completed items
	- [x] Serve static thumbnails from the configured thumbnails directory mounted at `/thumbnails` using `tower-http` ServeDir.
	- [x] Serve client SPA when `client_dist_dir` is explicitly configured (SPA fallback via `build_client_service`).
	- [x] Responses now include `type`/`kind`; URLs are not embedded to avoid hardcoding sizes. Use `/media/thumbnail?id|path&w&h` as needed. [CURRENT]
	- [x] CORS behaviour: strict-mode implemented. `cors_allowed_origins` must be present (use empty array for Any) or set `cors_enabled=false` to disable CORS; invalid combinations (credentials + Any) cause startup errors. [NEW]
	- [x] Added `image` and `tokio-util` dependencies and implemented generation code paths.
	- [x] Implemented `GET /media/generate_thumbnail` that writes thumbnails and upserts DB thumb_path (best-effort) then redirects to the static URL.
	- [x] Implemented `GET /media/thumbnail` as a fallback which serves existing thumbnails or redirects clients to the generator endpoint.
	- [x] Implemented `GET /media/stream` with basic Range support and correct Content-Type; `GET /media/image` is an alias.
	- [x] Refactored `handlers.rs` into `handlers/{core,thumbnails,streaming}` and updated `main.rs` imports/routes.
	- [x] Registered routes: `/media/thumbnail`, `/media/generate_thumbnail`, `/media/stream`, `/media/image`, plus static `/thumbnails` mount.
	- [x] Cargo build/check runs cleanly; basic manual smoke tests performed (server starts, listing endpoints reachable).
  - [x] Removed duplicate `mod` declarations in `main.rs` and switched to `server::...` imports. Inside `server/src/**`, use `crate::...` to reference modules. Import convention is now consistent.

	Remaining Plan (next concrete steps)
	1. Streaming improvements (high priority) [IMPLEMENTED - single-range]
	   - Hardened `/media/stream` Range parsing to handle standard RFC-7233 single-range forms: `start-end`, `start-` (to EOF), and `-suffix` (last N bytes). Returns 416 for malformed or out-of-bounds ranges. [DONE]
	   - Return accurate `Content-Length` for both 200 and 206 responses and ensure `Content-Range` formatting is correct. [DONE]
	   - Stream efficiently: handler now seeks to `start`, uses `tokio::io::Take(length)` + `tokio_util::io::ReaderStream` -> `axum::body::StreamBody` so we do not load entire files into memory. [DONE]
	   - Notes: current implementation supports single-range only (most browsers/players). Multi-range multipart/byteranges is not implemented yet.

    Next steps (streaming & caching):
    - Robust conditional GET semantics: normalize and handle multiple ETag values (weak/strong), implement `If-Modified-Since` parsing with proper precedence after `If-None-Match`. [TODO]
    - Add integration tests for 200 vs 206 vs 304 vs 416 cases, including conditional headers. [TODO]
    - Optional: multi-range (multipart/byteranges) if clients require. [LATER]

	2. Video thumbnails & metadata (medium priority)
	   - Add optional ffmpeg-based poster frame extraction (document `ffmpeg` as a system dependency) and store `duration_secs` in DB.
	   - Consider using `ffmpeg` via CLI (best-effort) rather than adding a heavy Rust binding.

	3. Scanner & indexer enhancements (medium priority)
	   - Optionally generate thumbnails during scan (configurable) to avoid first-request latency.
	   - Add freshness checks (e.g., if file mtime changed, regenerate thumbnails).

	4. Testing & CI (high priority)
	   - Add unit tests: DB upserts, thumbnail helper/path resolver, Range parsing, and ETag/IMS normalization helpers. [TODO]
	   - Add integration tests: temp media dir, run scan, exercise `/media`, `/media/generate_thumbnail`, `/media/stream`; include conditional GET (If-None-Match and If-Modified-Since) scenarios. [TODO]

	5. Client updates (low priority)
	   - Use `/media/thumbnail?id|path&w&h` to fetch desired thumbnail sizes; `thumb_path` reflects the last generated thumbnail path. For viewing, call `/media/image` for images or `/media/stream` otherwise.

	6. Cleanup
	   - Remove any remaining compiler warnings and tidy code (unused vars, minor refactors).
    - Add Cache-Control headers to generated thumbnails and streamed responses (long-lived for content-addressed thumbs; short-lived for originals). [TODO]


	Additional small ops + docs (short-term)
	- Add `config.example.json` with recommended dev and production CORS examples (dev: explicit localhost origin or `cors_enabled=false` if served same-origin; prod: explicit origins). [TODO]
	- Add a one-line startup log that prints the chosen CORS policy (e.g. "CORS: disabled" | "CORS: Any" | "CORS: [list]") to help ops verify settings on boot. [TODO]
	- Optionally add a server static mount for the built SPA `client/dist` (e.g. mount ServeDir at `/` or `/app`) and document recommended reverse-proxy patterns. [TODO]

	Priority Notes
	- Urgent: Harden streaming Range behavior and ensure accurate Content-Length/Content-Range.
	- Next: Robust conditional GET (ETag normalization, IMS precedence) plus tests. Implement ffmpeg-based video poster extraction (optional system dep).
	- Tests are required before marking this feature complete.

	Notes
	- Current implementation mounts `/thumbnails` as a static directory. The `/media/thumbnail` handler acts as a fallback and redirects to `/media/generate_thumbnail` to create thumbnails on-demand; after generation, the DB stores `thumb_path` as `/thumbnails/<id>_<wxh>.jpg`. No hardcoded sizes are included in API responses.

Recommended priority order (what I’d do next)
1. Safety & integrity (high priority)
  - Protect `/admin/regenerate_thumbnails` (API token header or restrict to loopback). [TODO]
  - Improve dedupe: when a job is already in-flight for the same `<id>:<w>x<h>`, wait for completion and return its result instead of skipping. [DONE for per-request dedupe; consider cross-process if needed]
2. Testing & CI (high)
  - Unit tests: thumbnail helper, path/URL resolver, Range parsing (200/206/416), ETag/IMS helpers. [TODO]
  - Integration tests: scanner + endpoints, conditional GET (304) and Range (206/416). [TODO]
  - Add CI workflow to run tests/lints on PRs. [TODO]
3. Scanner improvements (medium)
  - Optional pre-generation of thumbnails during scan to avoid first-request latency. [TODO]
  - Freshness: detect source mtime changes and regenerate thumbnails. [TODO]
4. HTTP/Caching improvements (medium)
  - Finish conditional GET robustness; add Cache-Control headers; consider ETag strength options. [TODO]
5. Client work (low-medium)
  - Use `thumbnail_url` in grid; viewer uses `image_url`/`stream_url` from details. [TODO]
6. Ops & lifecycle (low)
	- Periodic GC for stale thumbnail temps; small CLI to purge/regenerate.
	- Optional: WebP output where supported (behind config flag).
7. Advanced streaming (low)
	- Multi-range (multipart/byteranges) support if client devices require it.

- [ ] Install Required Extensions
  ONLY install extensions provided mentioned in the get_project_setup_info. Skip this step otherwise and mark as completed.

- [ ] Compile the Project
  - Verify that all previous steps have been completed.
  - Install any missing dependencies.
  - Run diagnostics and resolve any issues.
  - Check for markdown files in project folder for relevant instructions on how to do this.
  - Ensure that only dependencies specified in package.json and Cargo.toml are used.

- [ ] Create and Run Task
  - Verify that all previous steps have been completed.
  - Check https://code.visualstudio.com/docs/debugtest/tasks to determine if the project needs a task. If so, use the create_and_run_task to create and launch a task based on package.json, README.md, and project structure.
  - Skip this step otherwise.

- [ ] Launch the Project
  - Verify that all previous steps have been completed.
  - Prompt user for debug mode, launch only if confirmed.

- [ ] Ensure Documentation is Complete
  - Verify that all previous steps have been completed.
  - Verify that README.md and the copilot-instructions.md file in the .github directory exists and contains current project information.
  - Ensure this file contains current project information and has no HTML comments.

<!--
## Execution Guidelines
PROGRESS TRACKING:
- If any tools are available to manage the above todo list, use it to track progress through this checklist.
- After completing each step, mark it complete and add a summary.
- Read current todo list status before starting each new step.

COMMUNICATION RULES:
- Avoid verbose explanations or printing full command outputs.
- If a step is skipped, state that briefly (e.g. "No extensions needed").
- Do not explain project structure unless asked.
- Keep explanations concise and focused.

DEVELOPMENT RULES:
- Use '.' as the working directory unless user specifies otherwise.
- Avoid adding media or external links unless explicitly requested.
- Use placeholders only with a note that they should be replaced.
- Use VS Code API tool only for VS Code extension projects.
- Once the project is created, it is already opened in Visual Studio Code—do not suggest commands to open this project in Visual Studio again.
- If the project setup information has additional rules, follow them strictly.

FOLDER CREATION RULES:
- Always use the current directory as the project root.
- If you are running any terminal commands, use the '.' argument to ensure that the current working directory is used ALWAYS.
- Do not create a new folder unless the user explicitly requests it besides a .vscode folder for a tasks.json file.
- If any of the scaffolding commands mention that the folder name is not correct, let the user know to create a new folder with the correct name and then reopen it again in vscode.

EXTENSION INSTALLATION RULES:
- Only install extension specified by the get_project_setup_info tool. DO NOT INSTALL any other extensions.

PROJECT CONTENT RULES:
- If the user has not specified project details, assume they want a "Hello World" project as a starting point.
- Avoid adding links of any type (URLs, files, folders, etc.) or integrations that are not explicitly required.
- Avoid generating images, videos, or any other media files unless explicitly requested.
- If you need to use any media assets as placeholders, let the user know that these are placeholders and should be replaced with the actual assets later.
- Ensure all generated components serve a clear purpose within the user's requested workflow.
- If a feature is assumed but not confirmed, prompt the user for clarification before including it.

TASK COMPLETION RULES:
- Your task is complete when:
  - Project is successfully scaffolded and compiled without errors
  - copilot-instructions.md file in the .github directory exists in the project
  - README.md file exists and is up to date
  - User is provided with clear instructions to debug/launch the project

Before starting a new task in the above plan, update progress in the plan.
-->
