#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ROOT/orchestrator/pyproject.toml" ]]; then
  printf '%s\n' "SKIP orchestrator: orchestrator/pyproject.toml is not initialized."
  exit 0
fi

cd "$ROOT/orchestrator"
if [[ -x ".venv/bin/python" ]]; then
  python_command=".venv/bin/python"
elif command -v python3 >/dev/null 2>&1; then
  python_command="python3"
else
  printf '%s\n' "[HUMAN REQUIRED] Install Python before running orchestrator checks."
  exit 1
fi

"$python_command" -m pip check
diff -u requirements-dev.lock <("$python_command" -m pip freeze --exclude-editable 2>/dev/null)
"$python_command" -m ruff check .
"$python_command" -m mypy src tests
"$python_command" -m pytest

if [[ -f "scripts/run_graph_demo.py" ]]; then
  PYTHONPATH=src "$python_command" scripts/run_graph_demo.py
else
  printf '%s\n' "SKIP orchestrator graph demo: orchestrator/scripts/run_graph_demo.py is not initialized."
fi
