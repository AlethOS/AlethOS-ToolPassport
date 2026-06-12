# AlethOS ToolPassport

AlethOS ToolPassport is a structured audit index for AI tools. It groups
long-horizon, standard-driven audit runs around stable tool identities, binds
findings to evidence, preserves audit provenance, and produces hash-stable
records without claiming absolute truth.

The repository is an MVP monorepo with a Rust trust core, a LangGraph
orchestrator, a Next.js dashboard, and a minimal Foundry registry. The target
workflow and its migration from the current scaffold are documented in
`docs/project-overview.md` and `docs/technical-design.md`.

## Start Here

```bash
make init
make bootstrap
make doctor
make check
```

`make bootstrap` installs project dependencies using the configured Cargo,
PyPI, and npm registries. It prints the selected Python and npm sources and
supports `ALETHOS_PYPI_INDEX_URL` and `ALETHOS_NPM_REGISTRY` overrides. Trusted
HTTPS mirrors are acceptable; lock files and project checks remain required.
The script does not read `.env`, install Foundry, start services, or perform
wallet or chain operations.

Run individual development services with:

```bash
cd backend && cargo run
cd dashboard && npm run dev -- --hostname 127.0.0.1
cd orchestrator && PYTHONPATH=src .venv/bin/python scripts/run_graph_demo.py
```

The backend defaults to `sqlite://../data/toolpassport.db` when started from
`backend/`; set `DATABASE_URL` to override it. SQLx runs embedded SQLite
migrations on startup. The current Trust Core slice implements:

- `POST /api/runs` — create an audit run bound to an existing Tool (accepts `tool_id`)
- `GET /api/runs`
- `GET /api/runs/:run_id`
- `POST /api/runs/:run_id/events`
- `POST /api/tools` — create a tool candidate
- `GET /api/tools` — list all tools
- `GET /api/tools/by-id?tool_id=...` — get tool by namespaced ID
- `POST /api/tools/resolve` — three-state identity resolution
- `POST /api/tools/identifiers?tool_id=...` — approved source migration

Run events are append-only at both the API and SQLite trigger layers. The
backend atomically creates the initial `run_created` event and projects
validated node, approval, and terminal-status events into the Run summary.
The repository also includes a versioned core Audit Standard plus `generic`,
`agent_framework`, `mcp_server`, and `cli_api_tool` Profile fixtures. They are
validated offline against strict JSON Schemas and catalog cross-reference
rules by:

```bash
scripts/check_schemas.sh
```

Stage 2.1 adds strict versioned Tool Identity contracts, offline resolution
fixtures, and a standard-library Python reference normalizer. The resolver
uses only canonical strong identifiers, returns `resolved`,
`create_candidate`, or `needs_review`, and never performs network requests or
uses aliases for automatic merging.

Stage 2.2 implements the Rust Tool Registry persistence and API with SQLite
tables (`tools`, `tool_external_ids`, `tool_aliases`), CRUD endpoints,
three-state identity resolution ported from Python to pure Rust, and
approved source migration. External identifier uniqueness is enforced at the
database level via a composite primary key.

Stage 2.3 binds Run creation to the Tool Registry: `POST /api/runs` now
accepts a `tool_id` that must reference an existing Tool, and freezes the
canonical URL, name, and type as an immutable audit snapshot. A new
`runs.tool_id` column (migration `0003`) ensures every new Run is anchored to
a stable tool identity.

The Dashboard now provides a responsive, bilingual Trust Control Desk built
with Next.js, Tailwind CSS, TanStack Query, React Flow, and Lucide. It reads
authoritative health, Run, and append-only Event data through read-only
same-origin proxy routes. Audit results, Evidence Board, scores, commitments,
execution graph, and provenance views are explicitly labeled Preview because
their final Rust-backed contracts are not implemented yet. The Dashboard does
not calculate scores or Hashes and exposes no approval or chain-write action.

The minimal Foundry contract groups commitments by `toolId -> runId` and
records a Passport Hash, Audit Log Hash, and Evidence Manifest Hash. The
runtime Profile selector, orchestrator subprocess, SSE, evidence, artifacts,
passports, approval records, and onchain writes are not implemented yet.

Product scope and architecture are tracked in:

- `docs/project-overview.md`
- `docs/technical-design.md`
- `docs/testnet-setup.md`
- `AGENTS.md` and the nearest module-level `AGENTS.md`

`.codex/work-guide.md` is local Agent guidance and is intentionally ignored by
Git. A maintainer must provide it in each development workspace.

## Human-Owned Setup

The AI agent must report every required human operation as `[HUMAN REQUIRED]`.
Typical examples are:

- authorize GitHub, Browser, security, CI, or deployment integrations;
- install missing local toolchains and approve dependency downloads;
- create and populate `.env` without exposing it to the agent;
- approve paid API usage, wallet signing, testnet deployment, and onchain
  writes.

Mainnet use and unattended execution of audited third-party projects are
prohibited.

## GitHub CI

`.github/workflows/ci.yml` runs backend, orchestrator, dashboard, contract,
schema, and Markdown checks on every push and pull request. After the first
push, protect `main` and require all CI jobs before merging.

Deployment is intentionally not automated yet. Any future CD workflow must use
a protected GitHub environment and retain explicit human approval for wallet
signing, contract deployment, or onchain writes.
