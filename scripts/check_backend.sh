#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ROOT/backend/Cargo.toml" ]]; then
  printf '%s\n' "SKIP backend: backend/Cargo.toml is not initialized."
  exit 0
fi

command -v cargo >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Rust/Cargo before running backend checks."
  exit 1
}

cd "$ROOT/backend"
cargo fmt --check
cargo check
cargo test
cargo clippy -- -D warnings
