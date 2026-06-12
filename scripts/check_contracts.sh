#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -d "$HOME/.foundry/bin" ]]; then
  export PATH="$HOME/.foundry/bin:$PATH"
fi

if [[ ! -f "$ROOT/contracts/foundry.toml" ]]; then
  printf '%s\n' "SKIP contracts: contracts/foundry.toml is not initialized."
  exit 0
fi

command -v forge >/dev/null 2>&1 || {
  printf '%s\n' "[HUMAN REQUIRED] Install Foundry before running contract checks."
  exit 1
}

cd "$ROOT/contracts"
forge fmt --check
forge build
forge test -vvv
