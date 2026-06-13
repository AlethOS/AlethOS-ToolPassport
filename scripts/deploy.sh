#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="$PROJECT_ROOT/backend"
ORCHESTRATOR_DIR="$PROJECT_ROOT/orchestrator"
DASHBOARD_DIR="$PROJECT_ROOT/dashboard"

echo ">>> Starting deployment at $(date)"

echo ">>> Pulling latest code from GitHub..."
cd "$PROJECT_ROOT"
git checkout main
git pull --ff-only origin main

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

if systemctl list-unit-files | grep -q toolpassport-backend.service; then
  echo ">>> Restarting toolpassport-backend service..."
  systemctl restart toolpassport-backend
else
  echo "WARNING: toolpassport-backend.service not found. Please refer to deployment-guide.md"
fi

if command -v pm2 &> /dev/null; then
  echo ">>> Restarting dashboard with PM2..."
  cd "$DASHBOARD_DIR"
  pm2 restart toolpassport-dashboard || pm2 start npm --name "toolpassport-dashboard" -- run start -- -p 3000
  cd "$PROJECT_ROOT"
else
  echo "WARNING: pm2 not found. Frontend restart skipped."
fi

echo ">>> Deployment finished successfully!"
