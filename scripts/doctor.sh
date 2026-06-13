#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"
failures=0

if [[ -d "$HOME/.foundry/bin" ]]; then
  export PATH="$HOME/.foundry/bin:$PATH"
fi

ok() {
  printf 'OK                 %s\n' "$1"
}

skip() {
  printf 'SKIP               %s\n' "$1"
}

human() {
  printf '[HUMAN REQUIRED]   %s\n' "$1"
}

fail() {
  printf 'FAIL               %s\n' "$1"
  failures=$((failures + 1))
}

require_command() {
  local command_name="$1"
  local purpose="$2"
  if command -v "$command_name" >/dev/null 2>&1; then
    ok "$command_name: $purpose"
  else
    fail "Install $command_name: $purpose"
  fi
}

recommend_command() {
  local command_name="$1"
  local purpose="$2"
  if command -v "$command_name" >/dev/null 2>&1; then
    ok "$command_name: $purpose"
  else
    human "Install $command_name before using $purpose."
  fi
}

manifest_tool() {
  local manifest="$1"
  local command_name="$2"
  local purpose="$3"
  if [[ ! -f "$ROOT/$manifest" ]]; then
    skip "$manifest not initialized"
  elif command -v "$command_name" >/dev/null 2>&1; then
    ok "$command_name available for $purpose"
  else
    fail "$manifest exists but $command_name is missing for $purpose"
  fi
}

printf '%s\n' "AlethOS ToolPassport workspace doctor"
printf '%s\n' "Root: $ROOT"
printf '\n%s\n' "Core workspace"

require_command git "version control"
require_command bash "project scripts"
require_command make "uniform task entry points"
require_command jq "JSON syntax checks"
require_command codex "AI-assisted engineering workflow"

if git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  ok "Git repository initialized"
else
  fail "Git repository is not initialized"
fi

for required_file in \
  "AGENTS.md" \
  "docs/project-overview.md" \
  "docs/technical-design.md" \
  ".codex/work-guide.md"; do
  if [[ -f "$ROOT/$required_file" ]]; then
    ok "$required_file present"
  else
    fail "$required_file missing"
  fi
done

printf '\n%s\n' "Module toolchains"
manifest_tool "backend/Cargo.toml" cargo "backend checks"
manifest_tool "orchestrator/pyproject.toml" python3 "orchestrator checks"
manifest_tool "schemas/requirements.lock" python3 "JSON Schema meta-validation"
manifest_tool "dashboard/package.json" npm "dashboard checks"
manifest_tool "contracts/foundry.toml" forge "contract checks"
recommend_command forge "Foundry contract development"
recommend_command shellcheck "shell script linting"

if [[ -x "$ROOT/node_modules/.bin/markdownlint-cli2" ]]; then
  ok "markdownlint-cli2 available for Markdown linting"
else
  fail "Run make bootstrap to install markdownlint-cli2"
fi

if [[ -f "$ROOT/backend/Cargo.toml" ]]; then
  [[ -f "$ROOT/backend/Cargo.lock" ]] \
    && ok "backend/Cargo.lock present" \
    || fail "Run make bootstrap to generate backend/Cargo.lock"
  cargo clippy --version >/dev/null 2>&1 \
    && ok "cargo clippy available" \
    || fail "Run make bootstrap to install cargo clippy"
fi

if [[ -f "$ROOT/orchestrator/pyproject.toml" ]]; then
  [[ -f "$ROOT/orchestrator/requirements-dev.lock" ]] \
    && ok "orchestrator/requirements-dev.lock present" \
    || fail "Generate orchestrator/requirements-dev.lock before development"
  if [[ -x "$ROOT/orchestrator/.venv/bin/python" ]] \
    && "$ROOT/orchestrator/.venv/bin/python" -c "import langgraph, pydantic, pytest" >/dev/null 2>&1; then
    ok "orchestrator local dependencies installed"
    python_index="$("$ROOT/orchestrator/.venv/bin/python" -m pip config get global.index-url 2>/dev/null || true)"
    if [[ -n "$python_index" ]]; then
      ok "Python package source: $python_index"
    else
      ok "Python package source: pip default (https://pypi.org/simple)"
    fi
  else
    fail "Run make bootstrap to install orchestrator local dependencies"
  fi
fi

if [[ -f "$ROOT/schemas/requirements.lock" ]]; then
  if [[ -x "$ROOT/orchestrator/.venv/bin/python" ]] \
    && "$ROOT/orchestrator/.venv/bin/python" -c "import jsonschema" >/dev/null 2>&1; then
    ok "JSON Schema meta-validator installed"
  else
    fail "Run make bootstrap to install JSON Schema meta-validation dependencies"
  fi
fi

if [[ -f "$ROOT/dashboard/package.json" ]]; then
  [[ -x "$ROOT/dashboard/node_modules/.bin/next" ]] \
    && ok "dashboard local dependencies installed" \
    || fail "Run make bootstrap to install dashboard local dependencies"
  [[ -f "$ROOT/dashboard/package-lock.json" ]] \
    && ok "dashboard/package-lock.json present" \
    || fail "Run make bootstrap to generate dashboard/package-lock.json"
  npm_registry="$(cd "$ROOT/dashboard" && npm config get registry 2>/dev/null || true)"
  [[ -n "$npm_registry" ]] && ok "npm package source: $npm_registry"
fi

printf '\n%s\n' "AI engineering capabilities"
if [[ -f "$CODEX_HOME_DIR/skills/.system/openai-docs/SKILL.md" ]]; then
  ok "official openai-docs Skill installed"
else
  human "Install the official openai-docs Skill."
fi

if [[ -f "$CODEX_HOME_DIR/skills/security-threat-model/SKILL.md" ]]; then
  ok "official security-threat-model Skill installed"
else
  human "Install the official security-threat-model Skill."
fi

human "Verify GitHub Plugin authorization before private-repository or PR work."
human "Verify Context7 availability before implementation that depends on current library APIs."
human "Install or enable Browser/Playwright before visual Dashboard verification."
human "Install or enable Codex Security and CodeRabbit/CircleCI only when their review or CI workflow is needed."
human "Restart Codex after installing or updating Skills or Plugins."

printf '\n'
if (( failures > 0 )); then
  printf 'Doctor found %d blocking issue(s).\n' "$failures"
  exit 1
fi

printf '%s\n' "Doctor found no blocking issues for the currently initialized modules."
