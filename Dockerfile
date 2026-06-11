# Single-container build for known-chess, for CapRover (build context = repo root).
#
# Bundles two processes:
#   - kc-server (Rust)  — serves /api on an internal port (8080)
#   - the SvelteKit frontend (Node) — the only externally exposed port (3000),
#     which proxies /api to kc-server (see frontend/src/hooks.server.ts).
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

# --- Stage 2: build the SvelteKit frontend ----------------------------------
FROM node:24-slim AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install
COPY frontend/ ./
RUN npm run build

# --- Stage 3: runtime -------------------------------------------------------
# Node base (Debian/glibc) so the glibc-linked Rust binary runs as-is.
FROM node:24-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production

# Rust server binary.
COPY --from=rust-builder /app/target/release/kc-server /usr/local/bin/kc-server

# Frontend (built output + production deps).
COPY --from=frontend-builder /app/frontend/build ./frontend/build
COPY --from=frontend-builder /app/frontend/node_modules ./frontend/node_modules
COPY --from=frontend-builder /app/frontend/package.json ./frontend/package.json

COPY docker/start.sh /usr/local/bin/start.sh
RUN chmod +x /usr/local/bin/start.sh

# kc-server reads the book from KC_BOOK_PATH; mount a volume at /data and put
# the book there (or set KC_BOOK_PATH to wherever it lives).
ENV KC_BOOK_PATH=/data/book.book
# kc-server's internal address; the frontend proxies /api here.
ENV KC_BIND=127.0.0.1:8080
ENV KC_SERVER_URL=http://127.0.0.1:8080
# Port the frontend (and therefore the container) listens on. CapRover maps this.
ENV PORT=3000

EXPOSE 3000
CMD ["/usr/local/bin/start.sh"]
