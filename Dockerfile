# syntax=docker/dockerfile:1.7

FROM rust:1.96-bookworm AS backend-builder

WORKDIR /app/backend
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
COPY standards/ /app/standards/
COPY profiles/ /app/profiles/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/backend/target \
    cargo build --locked --release \
    && cp target/release/toolpassport-backend /tmp/toolpassport-backend

FROM node:22-bookworm-slim AS dashboard-builder

WORKDIR /app/dashboard
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci
COPY dashboard/ ./
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

FROM node:22-bookworm-slim AS runtime

RUN mkdir -p /app/data /app/runs \
    && chown -R node:node /app

COPY --from=backend-builder /tmp/toolpassport-backend /usr/local/bin/toolpassport-backend
COPY --from=dashboard-builder --chown=node:node /app/dashboard/.next/standalone /app/
COPY --from=dashboard-builder --chown=node:node /app/dashboard/.next/static /app/.next/static

COPY <<-"EOF" /usr/local/bin/start.sh
#!/bin/bash

cleanup() {
    echo "Shutting down services..."
    kill "$BACKEND_PID" "$DASHBOARD_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
    wait "$DASHBOARD_PID" 2>/dev/null || true
    echo "Shutdown complete."
}

trap cleanup SIGTERM SIGINT SIGQUIT

echo "Starting toolpassport-backend (port 8080)..."
toolpassport-backend &
BACKEND_PID=$!

echo "Starting dashboard (port 3000)..."
node /app/server.js &
DASHBOARD_PID=$!

echo "Both services running. Backend PID=$BACKEND_PID Dashboard PID=$DASHBOARD_PID"

# Wait for either child to exit; if one dies, shut down the other
wait -n 2>/dev/null || true
echo "A service exited. Initiating shutdown..."
cleanup
EOF

RUN chmod +x /usr/local/bin/start.sh

WORKDIR /app
USER node

ENV DATABASE_URL=sqlite:///app/data/toolpassport.db \
    ARTIFACT_ROOT=/app/runs \
    BIND_ADDR=0.0.0.0:8080 \
    RUST_LOG=toolpassport_backend=info \
    HOSTNAME=0.0.0.0 \
    PORT=3000 \
    NEXT_PUBLIC_BACKEND_URL=http://127.0.0.1:8080 \
    NEXT_TELEMETRY_DISABLED=1 \
    NODE_ENV=production

VOLUME ["/app/data", "/app/runs"]
EXPOSE 8080 3000

CMD ["/usr/local/bin/start.sh"]
