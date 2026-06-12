# Orchestrator Agent Instructions

- Use Python, LangGraph, and typed Pydantic state.
- Every node has typed input and output and emits start and finish events
  through the Rust backend.
- Validate every LLM structured output before use.
- Route invalid JSON to bounded repair and weak evidence to bounded research.
- Route signing or attestation to human approval; never write the database or
  chain directly.
- Before finishing, run `scripts/check_orchestrator.sh`.
