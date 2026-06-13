# syntax=docker/dockerfile:1.7

FROM rust:1.96-bookworm AS backend-builder

WORKDIR /app/backend
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/backend/target \
    cargo build --locked --release \
    && cp target/release/toolpassport-backend /tmp/toolpassport-backend

FROM debian:bookworm-slim AS backend-runtime

RUN mkdir -p /app/data /app/runs \
    && chown -R 65532:65532 /app

COPY --from=backend-builder /tmp/toolpassport-backend /usr/local/bin/toolpassport-backend

WORKDIR /app
USER 65532:65532

ENV DATABASE_URL=sqlite:///app/data/toolpassport.db \
    ARTIFACT_ROOT=/app/runs \
    BIND_ADDR=0.0.0.0:8080 \
    RUST_LOG=toolpassport_backend=info

VOLUME ["/app/data", "/app/runs"]
EXPOSE 8080

CMD ["toolpassport-backend"]

FROM node:22-bookworm-slim AS dashboard-builder

WORKDIR /app/dashboard
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci
COPY dashboard/ ./
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

FROM node:22-bookworm-slim AS dashboard-runtime

WORKDIR /app
ENV HOSTNAME=0.0.0.0 \
    PORT=3000 \
    NEXT_PUBLIC_BACKEND_URL=http://127.0.0.1:8080 \
    NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production

COPY --from=dashboard-builder --chown=node:node /app/dashboard/.next/standalone ./
COPY --from=dashboard-builder --chown=node:node /app/dashboard/.next/static ./.next/static

USER node
EXPOSE 3000
CMD ["node", "server.js"]

FROM backend-runtime AS runtime
