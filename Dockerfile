# Backend-only build for known-chess, for CapRover (build context = repo root).
#
# Runs kc-server (Rust), which serves /api with permissive CORS. The frontend
# is deployed separately to Cloudflare Pages (see frontend/wrangler.toml) and
# calls this server cross-origin via PUBLIC_KC_API_URL.
#
# The book is NOT baked into the image. Mount it as a volume and point
# KC_BOOK_PATH at it (defaults to /data/book.book).

# --- Stage 1: build the Rust server -----------------------------------------
FROM rust:1.94-slim AS rust-builder
WORKDIR /app

# Cache dependency compilation: copy manifests first, stub the sources.
COPY Cargo.toml Cargo.lock ./
COPY crates/shared/Cargo.toml crates/shared/Cargo.toml
COPY crates/processor/Cargo.toml crates/processor/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
RUN mkdir -p crates/shared/src crates/processor/src crates/server/src \
	&& echo 'pub fn _stub() {}' > crates/shared/src/lib.rs \
	&& echo 'fn main() {}' > crates/processor/src/main.rs \
	&& echo 'fn main() {}' > crates/server/src/main.rs \
	&& cargo build --release --bin kc-server || true

# Now copy the real sources and build for real.
COPY crates ./crates
RUN touch crates/*/src/*.rs && cargo build --release --bin kc-server

# --- Stage 2: runtime --------------------------------------------------------
FROM debian:bookworm-slim AS runtime
WORKDIR /app

COPY --from=rust-builder /app/target/release/kc-server /usr/local/bin/kc-server

# kc-server reads the book from KC_BOOK_PATH; mount a volume at /data and put
# the book there (or set KC_BOOK_PATH to wherever it lives).
ENV KC_BOOK_PATH=/data/book.book
# Port the container listens on. CapRover maps this.
ENV KC_BIND=0.0.0.0:8080

EXPOSE 8080
CMD ["kc-server"]
