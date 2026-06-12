#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shopt -s nullglob
schemas=("$ROOT"/schemas/*.json)

if (( ${#schemas[@]} == 0 )); then
  printf '%s\n' "SKIP schemas: no JSON Schema files are initialized."
  exit 0
fi

command -v jq >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install jq before running schema syntax checks."
  exit 1
}

for schema in "${schemas[@]}"; do
  jq -e . "$schema" >/dev/null
  printf 'PASS %s\n' "${schema#"$ROOT/"}"
done
