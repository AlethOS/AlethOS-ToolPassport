#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ROOT/dashboard/package.json" ]]; then
  printf '%s\n' "SKIP dashboard: dashboard/package.json is not initialized."
  exit 0
fi

command -v npm >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Node.js/npm before running dashboard checks."
  exit 1
}

cd "$ROOT/dashboard"
npm run lint
npm test
npm run build
