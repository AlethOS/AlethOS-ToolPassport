#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$ROOT/data" "$ROOT/runs"

printf '%s\n' "Workspace directories initialized."
printf '%s\n' "[HUMAN REQUIRED] Create and populate .env only when a real integration needs it; do not expose it to the AI agent."
"$ROOT/scripts/doctor.sh"
