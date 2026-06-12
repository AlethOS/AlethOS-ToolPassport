#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checks=(
  "check_backend.sh"
  "check_orchestrator.sh"
  "check_dashboard.sh"
  "check_contracts.sh"
  "check_schemas.sh"
  "check_docs.sh"
)

for check in "${checks[@]}"; do
  printf '\n==> %s\n' "$check"
  "$ROOT/scripts/$check"
done

printf '\n%s\n' "All initialized-module checks passed."
