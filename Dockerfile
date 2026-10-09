# syntax=docker/dockerfile:1
# Production image: one static-ish binary on distroless, running as non-root.

FROM rust:1.97-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p akasha && cp target/release/akasha /akasha

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /akasha /usr/local/bin/akasha
ENV AKASHA_BIND_ADDR=0.0.0.0:8080 AKASHA_LOG_FORMAT=json
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/akasha"]
CMD ["serve"]
