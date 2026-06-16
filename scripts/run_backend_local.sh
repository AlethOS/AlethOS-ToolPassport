#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

load_dotenv() {
  local file="$1"
  local line key value

  [[ -f "$file" ]] || return 0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
    [[ "$line" == export\ * ]] && line="${line#export }"
    if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
      key="${BASH_REMATCH[1]}"
      value="${BASH_REMATCH[2]}"
      if [[ "$value" =~ ^\"(.*)\"$ || "$value" =~ ^\'(.*)\'$ ]]; then
        value="${BASH_REMATCH[1]}"
      fi
      if [[ -z "${!key+x}" ]]; then
        export "$key=$value"
      fi
    fi
  done < "$file"
}

load_dotenv "$ROOT/.env"

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
