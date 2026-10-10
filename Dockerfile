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
ARG TARGETARCH
# whisper.cpp (speech to text, ADR 0017) is compiled from source with cmake.
RUN apt-get update && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# Compile SQL macros against the checked-in .sqlx cache; no database at build time.
ENV SQLX_OFFLINE=true
# Use the bindings shipped with whisper-rs-sys (no libclang needed).
ENV WHISPER_DONT_GENERATE_BINDINGS=1
COPY . .
COPY --from=web /web/dist web/dist
# whisper.cpp (ggml) is compiled for one instruction set, never for the build machine
# (GGML_NATIVE=OFF). On x86-64 the image carries two builds: `akasha` (portable, ggml
# limited to SSE4.2: any x86-64 CPU since ~2008, including AVX-less NAS Celerons/Atoms)
# and `akasha-avx2` (AVX/AVX2/FMA/F16C/BMI2, Haswell 2013+). The portable one starts
# first and re-execs the fast one when the CPU supports it (crates/app/src/cpu.rs).
# whisper-rs-sys does not rebuild when GGML_* changes, hence the `cargo clean -p`.
# arm64 builds once: NEON is part of baseline ARMv8 (Raspberry Pi 4/5, Graviton, Ampere).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    set -eu; export GGML_NATIVE=OFF; \
    build() { \
      cargo clean --release -p whisper-rs-sys; \
      cargo build --release -p akasha --features embed-ui; \
    }; \
    if [ "${TARGETARCH:-amd64}" = "amd64" ]; then \
      (export AKASHA_BUILD_CPU_VARIANT=avx2 GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON \
        GGML_F16C=ON GGML_BMI2=ON GGML_SSE42=ON; build); \
      cp target/release/akasha /akasha-avx2; \
      (export AKASHA_BUILD_CPU_VARIANT=baseline GGML_AVX=OFF GGML_AVX2=OFF GGML_FMA=OFF \
        GGML_F16C=OFF GGML_BMI2=OFF GGML_SSE42=ON; build); \
    else \
      (export AKASHA_BUILD_CPU_VARIANT=armv8 GGML_CPU_ARM_ARCH=armv8-a; build); \
    fi; \
    cp target/release/akasha /akasha; \
    mkdir -p /empty-dir /opt/akasha; \
    if [ -f /akasha-avx2 ]; then cp /akasha-avx2 /opt/akasha/akasha-avx2; fi

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
# The AVX2 build on amd64 (empty directory on arm64).
COPY --from=build /opt/akasha/ /usr/local/bin/
COPY --from=onnxruntime /libonnxruntime.so /usr/local/lib/libonnxruntime.so
# Uploaded file contents (local storage backend). Mount a volume here.
COPY --from=build --chown=nonroot:nonroot /empty-dir /var/lib/akasha/storage
# ML models (OCR, embedding, rerank, Whisper; downloaded on first use or with
# `akasha models download`). Mount a volume to keep them.
COPY --from=build --chown=nonroot:nonroot /empty-dir /var/lib/akasha/models
ENV AKASHA_BIND_ADDR=0.0.0.0:8080 AKASHA_LOG_FORMAT=json AKASHA_STORAGE_DIR=/var/lib/akasha/storage \
    AKASHA_MODELS_DIR=/var/lib/akasha/models AKASHA_ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so
VOLUME ["/var/lib/akasha/storage", "/var/lib/akasha/models"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/akasha"]
# API and background worker in one process; run `akasha worker` separately to scale.
CMD ["serve", "--with-worker"]
