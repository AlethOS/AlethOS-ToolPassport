#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="$PROJECT_ROOT/backend"
ORCHESTRATOR_DIR="$PROJECT_ROOT/orchestrator"
DASHBOARD_DIR="$PROJECT_ROOT/dashboard"
BACKEND_SERVICE="${DEPLOY_BACKEND_SERVICE:-toolpassport-backend.service}"
DASHBOARD_PROCESS="${DEPLOY_DASHBOARD_PROCESS:-toolpassport-dashboard}"
DASHBOARD_PORT="${DEPLOY_DASHBOARD_PORT:-3000}"

echo ">>> Starting deployment at $(date)"

cd "$PROJECT_ROOT"
DEPLOY_REVISION="$(git rev-parse --verify HEAD)"
echo ">>> Deploying checked-out revision $DEPLOY_REVISION"

echo ">>> Setting up Python orchestrator..."
cd "$ORCHESTRATOR_DIR"
if command -v python3.13 &> /dev/null; then
  python3.13 -m venv .venv --upgrade-deps
  .venv/bin/python -m pip install -r requirements-dev.lock
  .venv/bin/python -m pip install -r ../schemas/requirements.lock
  .venv/bin/python -m pip install --no-deps -e .
else
  echo "ERROR: python3.13 is required for repository checks and deployment."
  exit 1
fi
cd "$PROJECT_ROOT"

echo ">>> Running repository checks..."
scripts/check_all.sh

echo ">>> Building Rust backend..."
cd "$BACKEND_DIR"
cargo build --release
cd "$PROJECT_ROOT"

echo ">>> Building Next.js dashboard..."
cd "$DASHBOARD_DIR"
npm ci
npm run build
cd "$PROJECT_ROOT"

echo ">>> Restarting services..."

if systemctl list-unit-files "$BACKEND_SERVICE" --no-legend 2>/dev/null | grep -q "$BACKEND_SERVICE"; then
  echo ">>> Restarting $BACKEND_SERVICE..."
  systemctl restart "$BACKEND_SERVICE"
else
  echo "WARNING: $BACKEND_SERVICE not found. Backend restart skipped."
fi

if command -v pm2 &> /dev/null; then
  echo ">>> Restarting $DASHBOARD_PROCESS with PM2..."
  cd "$DASHBOARD_DIR"
  pm2 restart "$DASHBOARD_PROCESS" || \
    pm2 start npm --name "$DASHBOARD_PROCESS" -- run start -- -p "$DASHBOARD_PORT"
  cd "$PROJECT_ROOT"
else
  echo "WARNING: pm2 not found. Frontend restart skipped."
fi

echo ">>> Deployment finished successfully!"
