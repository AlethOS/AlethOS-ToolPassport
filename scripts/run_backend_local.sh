#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ORCHESTRATOR_PYTHON="${ORCHESTRATOR_PYTHON:-$ROOT/orchestrator/.venv/bin/python}"

if [[ ! -x "$ORCHESTRATOR_PYTHON" ]]; then
  printf '%s\n' "[HUMAN REQUIRED] Initialize orchestrator/.venv before starting the local backend."
  exit 1
fi

mkdir -p "$ROOT/data" "$ROOT/runs"

export DATABASE_URL="${DATABASE_URL:-sqlite://$ROOT/data/toolpassport.db}"
export ARTIFACT_ROOT="${ARTIFACT_ROOT:-$ROOT/runs}"
export BIND_ADDR="${BIND_ADDR:-127.0.0.1:8080}"
export BACKEND_URL="${BACKEND_URL:-http://127.0.0.1:8080}"
export ORCHESTRATOR_PYTHON
export ORCHESTRATOR_DIR="${ORCHESTRATOR_DIR:-$ROOT/orchestrator}"
export ORCHESTRATOR_LIVE_RESEARCH="${ORCHESTRATOR_LIVE_RESEARCH:-true}"
export ORCHESTRATOR_CHECKPOINT_DB="${ORCHESTRATOR_CHECKPOINT_DB:-$ROOT/data/orchestrator-checkpoints.sqlite}"

exec cargo run --manifest-path "$ROOT/backend/Cargo.toml"
