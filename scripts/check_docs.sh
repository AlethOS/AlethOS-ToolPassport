#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

required_files=(
  "README.md"
  "AGENTS.md"
  "docs/project-overview.md"
  "docs/technical-design.md"
  "docs/testnet-setup.md"
  "backend/AGENTS.md"
  "orchestrator/AGENTS.md"
  "dashboard/AGENTS.md"
  "contracts/AGENTS.md"
  "schemas/AGENTS.md"
)

for required_file in "${required_files[@]}"; do
  if [[ ! -s "$ROOT/$required_file" ]]; then
    printf 'FAIL missing or empty: %s\n' "$required_file"
    exit 1
  fi
  printf 'PASS %s\n' "$required_file"
done

markdownlint="$ROOT/node_modules/.bin/markdownlint-cli2"
if [[ ! -x "$markdownlint" ]]; then
  printf '%s\n' "FAIL markdownlint-cli2 is missing; run make bootstrap."
  exit 1
fi

cd "$ROOT"
"$markdownlint"

if [[ -s "$ROOT/.codex/work-guide.md" ]]; then
  printf '%s\n' "PASS .codex/work-guide.md (local-only)"
else
  printf '%s\n' "[HUMAN REQUIRED] Provide .codex/work-guide.md before broad Agent-driven development."
fi

if git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git -C "$ROOT" check-ignore -q .codex/work-guide.md || {
    printf '%s\n' "FAIL .codex/work-guide.md must remain local-only and ignored by Git."
    exit 1
  }
  git -C "$ROOT" check-ignore -q .env || {
    printf '%s\n' "FAIL .env must be ignored by Git."
    exit 1
  }
fi

printf '%s\n' "Documentation and Agent guidance checks passed."
