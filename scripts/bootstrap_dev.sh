#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

printf '%s\n' "Bootstrapping AlethOS ToolPassport development dependencies."
printf '%s\n' "This script does not read .env or perform wallet, deployment, or chain operations."

command -v cargo >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Rust/Cargo from the official Rust distribution."
  exit 1
}
command -v rustup >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install rustup from the official Rust distribution."
  exit 1
}
command -v python3 >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Python 3.11 or newer."
  exit 1
}
command -v npm >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Node.js and npm."
  exit 1
}

rustup component add clippy
cargo fetch --manifest-path "$ROOT/backend/Cargo.toml"

python3 -m venv "$ROOT/orchestrator/.venv"
python_index_args=()
if [[ -n "${ALETHOS_PYPI_INDEX_URL:-}" ]]; then
  python_index_args=(--index-url "$ALETHOS_PYPI_INDEX_URL")
  printf 'Python package source override: %s\n' "$ALETHOS_PYPI_INDEX_URL"
else
  printf '%s\n' "Python package source: inherited pip configuration"
  "$ROOT/orchestrator/.venv/bin/python" -m pip config get global.index-url 2>/dev/null || \
    printf '%s\n' "https://pypi.org/simple (pip default)"
fi
"$ROOT/orchestrator/.venv/bin/python" -m pip install "${python_index_args[@]}" --upgrade pip
"$ROOT/orchestrator/.venv/bin/python" -m pip install \
  "${python_index_args[@]}" \
  -r "$ROOT/orchestrator/requirements-dev.lock"
"$ROOT/orchestrator/.venv/bin/python" -m pip install \
  "${python_index_args[@]}" \
  --no-deps \
  -e "$ROOT/orchestrator"

cd "$ROOT/dashboard"
npm_registry="${ALETHOS_NPM_REGISTRY:-$(npm config get registry)}"
printf 'npm package source: %s\n' "$npm_registry"
npm install --registry="$npm_registry"

cd "$ROOT"
npm install --registry="$npm_registry"

if [[ ! -x "$HOME/.foundry/bin/forge" ]] && ! command -v forge >/dev/null 2>&1; then
  printf '%s\n' "[HUMAN REQUIRED] Install Foundry from its official distribution before contract development."
fi

"$ROOT/scripts/doctor.sh"
