# Backend Agent Instructions

- Use Rust and Axum.
- Keep API handlers thin; put business rules in services.
- Use typed request and response structs. JSON is the default response format.
- Rust owns persistence, append-only events, artifacts, stable hashes,
  approvals, and onchain submission.
- Never store secrets in the database or logs.
- Add focused tests for API behavior and trust-boundary rules.
- Before finishing, run `scripts/check_backend.sh`.
