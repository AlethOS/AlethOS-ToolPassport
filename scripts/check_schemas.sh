#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mapfile -d '' json_files < <(
  find "$ROOT/schemas" "$ROOT/standards" "$ROOT/profiles" "$ROOT/fixtures" \
    -type f -name '*.json' -print0 | sort -z
)

if (( ${#json_files[@]} == 0 )); then
  printf '%s\n' "SKIP schemas: no JSON files are initialized."
  exit 0
fi

command -v jq >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install jq before running schema syntax checks."
  exit 1
}

command -v python3 >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Python 3 before running schema contract checks."
  exit 1
}

for json_file in "${json_files[@]}"; do
  jq -e . "$json_file" >/dev/null
  printf 'PASS %s\n' "${json_file#"$ROOT/"}"
done

python3 "$ROOT/scripts/validate_audit_catalog.py"
python3 "$ROOT/scripts/validate_tool_identity.py"
python3 -m unittest discover -s "$ROOT/schemas/tests" -p 'test_*.py'
