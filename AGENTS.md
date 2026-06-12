# AlethOS ToolPassport Engineering Instructions

## Read First

Before editing, read:

1. `docs/project-overview.md` for product goals and MVP scope.
2. `docs/technical-design.md` for architecture and shared contracts.
3. `.codex/work-guide.md` for the local AI workflow and done criteria.
4. The nearest module-level `AGENTS.md` for local rules.

`.codex/work-guide.md` is intentionally local-only and ignored by Git. If it is
missing, report `[HUMAN REQUIRED]` before broad or architecture-changing work.

## Architecture Ownership

- Rust/Axum owns APIs, persistence, append-only events, artifacts, final
  hashes, approvals, and onchain submission.
- Python/LangGraph owns long-running orchestration, typed graph state,
  branching, retry limits, and approval waiting.
- GLM-5.1 owns reasoning outputs, which must be schema-validated before use.
- Next.js owns presentation and user actions only.
- Solidity/Foundry owns the minimal `ToolPassportRegistry`.
- Codex is a development tool, not a runtime trust component.

Do not duplicate or move these responsibilities without first updating
`docs/technical-design.md`.

## Hard Rules

- Do not read, edit, print, or commit `.env`.
- Do not commit secrets.
- Do not use mainnet.
- Do not install or execute unknown audited repositories.
- Do not allow wallet signing, contract deployment, or onchain writes without
  explicit human approval.
- Do not let the dashboard or orchestrator bypass the Rust backend for database
  or chain writes.
- Treat run events as append-only.
- Validate all LLM structured output.
- Keep changes scoped; do not rewrite unrelated modules.

## Human Action Protocol

- Identify credentials, authorization, paid services, policy decisions, wallet
  signing, deployment, onchain writes, and untrusted code execution before
  acting.
- Mark unresolved human-only work as `[HUMAN REQUIRED]`, including the reason,
  exact action, risk or decision, and resume condition.
- Stop before side effects that lack explicit approval.
- Include remaining human actions in the final task summary.

## Required Checks

Run relevant module checks after every change. Prefer:

```bash
scripts/check_backend.sh
scripts/check_orchestrator.sh
scripts/check_dashboard.sh
scripts/check_contracts.sh
scripts/check_schemas.sh
scripts/check_docs.sh
scripts/check_all.sh
```

Never claim a check passed if dependencies or tools were unavailable.

## GitHub CI/CD Policy

- `.github/workflows/ci.yml` must run on every pushed commit and pull request.
- Do not merge while any required CI job is failing or missing.
- Keep local and GitHub checks aligned through `scripts/`.
- Normal CI must not deploy, sign, or write onchain. Those operations require a
  protected environment and explicit human approval.

## Done Means

- The requested behavior works and has focused tests.
- Shared contracts stay consistent across modules.
- Relevant formatting, checks, and tests pass.
- Security boundaries and the mockable local demo path remain intact.
