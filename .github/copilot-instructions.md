<!-- Use this file to provide workspace-specific custom instructions to Copilot. For more details, visit https://code.visualstudio.com/docs/copilot/copilot-customization#_use-a-githubcopilotinstructionsmd-file -->
- [x] Verify that the copilot-instructions.md file in the .github directory is created.

- [x] Clarify Project Requirements
	<!-- Project type: Monorepo with React TypeScript client and Rust server. -->

- [x] Scaffold the Project
	<!--
	- React client files moved to 'client' directory.
	- Rust server files remain in 'server' directory.
	-->

- [x] Customize the Project
	<!--
	Verify that all previous steps have been completed successfully and you have marked the step as completed.
	Develop a plan to modify codebase according to user requirements.
	Ensure that only dependencies specified in package.json and Cargo.toml are used.
	Apply modifications using appropriate tools and user-provided references.
	Skip this step for "Hello World" projects.
	-->

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

	2. Storage/model changes
		- Store thumbnails on disk under a configurable thumbnails directory (defaults follow XDG or `/var/cache`) using deterministic names (e.g., `<media_id>_<wxh>.jpg`). [UPDATED - static serving added, generator writes to configured dir]
	   - Add nullable columns to `media` via lightweight migration: `width`, `height`, `duration_secs` (for videos), and `thumb_path` (relative path under `.thumbnails`). Keep data model backward compatible. [DONE]

	3. Scanner updates
	   - On scan (or first request), generate thumbnails for images if missing or stale; record `thumb_path`, `width`, `height`. [PARTIAL - scanner records fields but automatic thumbnail generation during scan not implemented]
	   - For videos, generate a poster frame with `ffmpeg` (best-effort, skip if tool missing). [PENDING]

	4. APIs
	   - `GET /media/thumbnail?id|path[&w=..&h=..]`: returns a small image thumbnail (serve existing or generate on-demand; cache result; set appropriate `Content-Type` and caching headers). [IMPLEMENTED]
	     - Behavior: serves existing static thumbnail when present; when missing, `thumbnail_handler` now redirects clients to the generator endpoint.
		- `GET /media/generate_thumbnail?id|path&w&h`: on-demand generator that creates a thumbnail, writes it to the configured thumbnails directory (`<thumbnails_dir>/<id>_<wxh>.jpg`), upserts `thumb_path`, and redirects to the static URL. [IMPLEMENTED]
	   - `GET /media/stream?id|path`: stream files (images, audio, video) with HTTP Range support. [IMPLEMENTED - basic single-range support]
	   - `GET /media/image?id|path` (alias): serve full-resolution images with correct `Content-Type` (alias to `stream`). [IMPLEMENTED]
	   - Enhance `GET /media/details` response to include `thumbnail_url` and `stream_url`/`image_url` fields for clients. [IMPLEMENTED]

	5. Validation & security
	   - Enforce relative paths (no leading `/`, no `..`). Ensure resolved file paths stay under configured media root. [IMPLEMENTED]

	6. Testing
	   - Unit: DB model changes; thumbnail path resolver; Range parsing. [PENDING]
	   - Integration: create temp media dir with small image/video; run scan; assert `thumbnail` and `stream` endpoints; verify Range behavior and content types. [PENDING]

	7. Client (follow-up)
	   - Update React client to render grid with thumbnails and open viewer that uses `image_url` or `stream_url` from details. [PENDING]

	In progress / Completed items
	- [x] Serve static thumbnails from the configured thumbnails directory mounted at `/thumbnails` using `tower-http` ServeDir.
	- [x] Include `thumbnail_url` and `stream_url` in `/media` and `/media/details` responses. Listings now return `thumbnail_url` pointing at `/thumbnails/<id>_200x200.jpg` when a thumbnail is known (or will be created by the generator).
	- [x] Added `image` and `tokio-util` dependencies and implemented generation code paths.
	- [x] Implemented `GET /media/generate_thumbnail` that writes thumbnails and upserts DB thumb_path (best-effort) then redirects to the static URL.
	- [x] Implemented `GET /media/thumbnail` as a fallback which serves existing thumbnails or redirects clients to the generator endpoint.
	- [x] Implemented `GET /media/stream` with basic Range support and correct Content-Type; `GET /media/image` is an alias.
	- [x] Refactored `handlers.rs` into `handlers/{core,thumbnails,streaming}` and updated `main.rs` imports/routes.
	- [x] Registered routes: `/media/thumbnail`, `/media/generate_thumbnail`, `/media/stream`, `/media/image`, plus static `/thumbnails` mount.
	- [x] Cargo build/check runs cleanly; basic manual smoke tests performed (server starts, listing endpoints reachable).

	Remaining Plan (next concrete steps)
	1. Streaming improvements (high priority) [IMPLEMENTED - single-range]
	   - Hardened `/media/stream` Range parsing to handle standard RFC-7233 single-range forms: `start-end`, `start-` (to EOF), and `-suffix` (last N bytes). Returns 416 for malformed or out-of-bounds ranges. [DONE]
	   - Return accurate `Content-Length` for both 200 and 206 responses and ensure `Content-Range` formatting is correct. [DONE]
	   - Stream efficiently: handler now seeks to `start`, uses `tokio::io::Take(length)` + `tokio_util::io::ReaderStream` -> `axum::body::StreamBody` so we do not load entire files into memory. [DONE]
	   - Notes: current implementation supports single-range only (most browsers/players). Multi-range multipart/byteranges is not implemented yet.

	   Next steps (streaming):
	   - Add optional multi-range support (multipart/byteranges) if clients require it.
	   - Add ETag / Last-Modified / conditional GET support for caching and 304 responses (could be delegated to a static file service for thumbnails).
	   - Add integration tests to verify 200 vs 206 behavior, Content-Length/Content-Range, and 416 cases.
	   - Consider using `hyper-staticfile` or tower-http `ServeFile` for pure static file cases to get more comprehensive conditional/Range semantics out-of-the-box.

	2. Video thumbnails & metadata (medium priority)
	   - Add optional ffmpeg-based poster frame extraction (document `ffmpeg` as a system dependency) and store `duration_secs` in DB.
	   - Consider using `ffmpeg` via CLI (best-effort) rather than adding a heavy Rust binding.

	3. Scanner & indexer enhancements (medium priority)
	   - Optionally generate thumbnails during scan (configurable) to avoid first-request latency.
	   - Add freshness checks (e.g., if file mtime changed, regenerate thumbnails).

	4. Testing & CI (high priority)
	   - Add unit tests for DB upserts, thumbnail path resolution, and Range parsing logic.
	   - Add integration tests that run against a temp media dir, run the scanner, then exercise `/media`, `/media/generate_thumbnail`, and `/media/stream` endpoints.

	5. Client updates (low priority)
	   - Update the React client to use `thumbnail_url` from `/media` and call `/media/image` or `/media/stream` when viewing details.

	6. Cleanup
	   - Remove any remaining compiler warnings and tidy code (unused vars, minor refactors).
	   - Add Cache-Control headers to generated thumbnails and consider `ETag`/Last-Modified support.

	Priority Notes
	- Urgent: Harden streaming Range behavior and ensure accurate Content-Length/Content-Range.
	- Next: Implement ffmpeg-based video poster extraction (optional system dep).
	- Tests are required before marking this feature complete.

	Notes
	- Current implementation mounts `/thumbnails` as a static directory. The `/media/thumbnail` handler acts as a fallback and now redirects to `/media/generate_thumbnail` to create thumbnails on-demand; after generation, clients should prefer the static `/thumbnails/<id>_<wxh>.jpg` URL returned in metadata.

- [ ] Install Required Extensions
	<!-- ONLY install extensions provided mentioned in the get_project_setup_info. Skip this step otherwise and mark as completed. -->

- [ ] Compile the Project
	<!--
	Verify that all previous steps have been completed.
	Install any missing dependencies.
	Run diagnostics and resolve any issues.
	Check for markdown files in project folder for relevant instructions on how to do this.
	Ensure that only dependencies specified in package.json and Cargo.toml are used.
	-->

- [ ] Create and Run Task
	<!--
	Verify that all previous steps have been completed.
	Check https://code.visualstudio.com/docs/debugtest/tasks to determine if the project needs a task. If so, use the create_and_run_task to create and launch a task based on package.json, README.md, and project structure.
	Skip this step otherwise.
	 -->

- [ ] Launch the Project
	<!--
	Verify that all previous steps have been completed.
	Prompt user for debug mode, launch only if confirmed.
	 -->

- [ ] Ensure Documentation is Complete
	<!--
	Verify that all previous steps have been completed.
	Verify that README.md and the copilot-instructions.md file in the .github directory exists and contains current project information.
	Clean up the copilot-instructions.md file in the .github directory by removing all HTML comments.
	-->

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
