# Schema Agent Instructions

- JSON Schema files are the machine-readable authority for cross-module
  contracts.
- Keep schemas strict, versioned, and backward-readable where required.
- Coordinate every shared field change with backend, orchestrator, dashboard,
  tests, and `docs/technical-design.md`.
- Do not trust LLM-provided totals, ratings, hashes, or identifiers that the
  Rust trust core must calculate.
- Before finishing, run `scripts/check_schemas.sh` and `scripts/check_all.sh`.
