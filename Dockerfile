# syntax=docker/dockerfile:1
# Production image: one static-ish binary on distroless, running as non-root.

FROM rust:1.97-bookworm AS build
WORKDIR /src
# Compile SQL macros against the checked-in .sqlx cache; no database at build time.
ENV SQLX_OFFLINE=true
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p akasha && cp target/release/akasha /akasha \
    && mkdir -p /empty-dir

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /akasha /usr/local/bin/akasha
# Uploaded file contents (local storage backend). Mount a volume here.
COPY --from=build --chown=nonroot:nonroot /empty-dir /var/lib/akasha/storage
ENV AKASHA_BIND_ADDR=0.0.0.0:8080 AKASHA_LOG_FORMAT=json AKASHA_STORAGE_DIR=/var/lib/akasha/storage
VOLUME ["/var/lib/akasha/storage"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/akasha"]
# API and background worker in one process; run `akasha worker` separately to scale.
CMD ["serve", "--with-worker"]
