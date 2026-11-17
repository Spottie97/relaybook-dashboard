# syntax=docker/dockerfile:1

FROM node:22-bookworm-slim AS ui
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1.88-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY fixtures ./fixtures
COPY --from=ui /src/frontend/dist ./frontend/dist
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends wget ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin dashboard
COPY --from=build /src/target/release/relaybook-dashboard /usr/local/bin/relaybook-dashboard
USER dashboard
ENV BIND=0.0.0.0:7878
EXPOSE 7878
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD wget -qO- http://127.0.0.1:7878/healthz || exit 1
ENTRYPOINT ["relaybook-dashboard"]
