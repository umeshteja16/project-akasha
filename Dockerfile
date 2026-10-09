# syntax=docker/dockerfile:1
# Production image: one static-ish binary (API + embedded web UI) on distroless,
# running as non-root.

# The web UI, built once and embedded into the binary (feature `embed-ui`).
FROM node:22-bookworm-slim AS web
WORKDIR /web
RUN npm install -g pnpm@10.28.0
COPY web/package.json web/pnpm-lock.yaml ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1.97-bookworm AS build
WORKDIR /src
# Compile SQL macros against the checked-in .sqlx cache; no database at build time.
ENV SQLX_OFFLINE=true
COPY . .
COPY --from=web /web/dist web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p akasha --features embed-ui && cp target/release/akasha /akasha \
    && mkdir -p /empty-dir

# ONNX Runtime for embeddings/reranking, loaded by akasha at run time (ADR 0009).
# Pinned release + SHA-256; must be >= the version the `ort` crate targets (1.28 for
# ort 2.0.0-rc.13). Debian-glibc build; needs only libstdc++, which distroless/cc has.
FROM rust:1.97-bookworm AS onnxruntime
ARG TARGETARCH
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
      amd64) arch=x64; sum=a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407 ;; \
      arm64) arch=aarch64; sum=e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb ;; \
      *) echo "unsupported architecture ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/ort.tgz \
      "https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-${arch}-1.28.0.tgz"; \
    echo "${sum}  /tmp/ort.tgz" | sha256sum -c -; \
    tar -xzf /tmp/ort.tgz -C /tmp; \
    cp "/tmp/onnxruntime-linux-${arch}-1.28.0/lib/libonnxruntime.so.1.28.0" /libonnxruntime.so

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /akasha /usr/local/bin/akasha
COPY --from=onnxruntime /libonnxruntime.so /usr/local/lib/libonnxruntime.so
# Uploaded file contents (local storage backend). Mount a volume here.
COPY --from=build --chown=nonroot:nonroot /empty-dir /var/lib/akasha/storage
# ML models (OCR, embedding, rerank; downloaded on first use or with
# `akasha models download`). Mount a volume to keep them.
COPY --from=build --chown=nonroot:nonroot /empty-dir /var/lib/akasha/models
ENV AKASHA_BIND_ADDR=0.0.0.0:8080 AKASHA_LOG_FORMAT=json AKASHA_STORAGE_DIR=/var/lib/akasha/storage \
    AKASHA_MODELS_DIR=/var/lib/akasha/models AKASHA_ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so
VOLUME ["/var/lib/akasha/storage", "/var/lib/akasha/models"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/akasha"]
# API and background worker in one process; run `akasha worker` separately to scale.
CMD ["serve", "--with-worker"]
