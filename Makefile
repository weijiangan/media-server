# Define directories for JavaScript packages and Rust crates
JS_DIRS := media-server-client
RUST_CRATES := server

# Default target
.PHONY: start
start: start-js start-rust

# Target to start all JavaScript packages
.PHONY: start-js
start-js:
	@for dir in $(JS_DIRS); do \
		( \
			echo "Starting Node.js package in $$dir..."; \
			cd $$dir && bun start; \
		) || exit 1; \
	done

# Target to start all Rust crates
.PHONY: start-rust
start-rust:
	@for crate in $(RUST_CRATES); do \
		( \
			echo "Starting Rust crate in $$crate..."; \
			cargo run --manifest-path $$crate/Cargo.toml; \
		) || exit 1; \
	done