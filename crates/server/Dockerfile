# Multi-stage build for the known-chess server.
# Build context is the repo root: `docker build -f crates/server/Dockerfile .`

FROM rust:1.94-slim AS builder
WORKDIR /app

# Cache dependency compilation: copy manifests first.
COPY Cargo.toml Cargo.lock ./
COPY crates/shared/Cargo.toml crates/shared/Cargo.toml
COPY crates/processor/Cargo.toml crates/processor/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
# Stub sources so the dependency graph resolves for the cache layer.
RUN mkdir -p crates/shared/src crates/processor/src crates/server/src \
	&& echo 'pub fn _stub() {}' > crates/shared/src/lib.rs \
	&& echo 'fn main() {}' > crates/processor/src/main.rs \
	&& echo 'fn main() {}' > crates/server/src/main.rs \
	&& cargo build --release --bin kc-server || true

# Now copy the real sources and build for real.
COPY crates ./crates
RUN touch crates/*/src/*.rs && cargo build --release --bin kc-server

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN useradd --system --uid 10001 kc
COPY --from=builder /app/target/release/kc-server /usr/local/bin/kc-server

ENV KC_BOOK_PATH=/data/book.book
ENV KC_BIND=0.0.0.0:8080
EXPOSE 8080
USER kc
ENTRYPOINT ["kc-server"]
