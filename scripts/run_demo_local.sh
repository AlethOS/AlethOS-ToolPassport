#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
missing=0

for manifest in \
  "backend/Cargo.toml" \
  "orchestrator/pyproject.toml" \
  "dashboard/package.json"; do
  if [[ ! -f "$ROOT/$manifest" ]]; then
    printf 'NOT READY          %s is not initialized.\n' "$manifest"
    missing=1
  fi
done

if (( missing > 0 )); then
  printf '%s\n' "[HUMAN REQUIRED] Choose and approve the module bootstrap/dependency installation before the local demo can run."
  exit 1
fi

printf '%s\n' "[HUMAN REQUIRED] The service supervisor for the local demo has not been implemented yet."
exit 1
