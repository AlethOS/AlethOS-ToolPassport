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
The existing orchestrator virtual environment also receives the pinned
`jsonschema` tooling from `schemas/requirements.lock`; schema checks do not
create a second Python virtual environment.
The script does not read `.env`, install Foundry, start services, or perform
wallet or chain operations.

Run individual development services with:

```bash
cd backend && cargo run
cd dashboard && npm run dev -- --hostname 127.0.0.1
cd orchestrator && PYTHONPATH=src .venv/bin/python scripts/run_graph_demo.py
```

The backend defaults to `sqlite://../data/toolpassport.db` when started from
`backend/`; set `DATABASE_URL` to override it. It listens on
`127.0.0.1:8080` by default; set `BIND_ADDR` to override it. SQLx runs embedded
SQLite migrations on startup. The current Trust Core slice implements:

- `POST /api/runs` — create an audit run bound to an existing Tool (accepts `tool_id`)
- `GET /api/runs`
- `GET /api/runs/:run_id`
- `POST /api/runs/:run_id/events`
- `POST /api/runs/:run_id/artifacts` — upload one bounded multipart artifact
- `GET /api/runs/:run_id/artifacts`
- `POST /api/runs/:run_id/evidence` — persist one normalized Evidence JSON object
- `GET /api/runs/:run_id/evidence`
- `POST /api/runs/:run_id/check-results` — validate findings, score, and persist immutable results
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
rules by the pinned Python `jsonschema` implementation. The check also validates
every committed `*.schema.json` against the Draft 2020-12 meta-schema:

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

Stage 3 provides a fully offline Pydantic/LangGraph investigation mock with
Profile selection, bounded research rounds, Evidence Board and Gap Tracker
state, stopping conditions, and Skeptic Review. It does not call real sources
or persist its mock Evidence through the backend yet.

Stage 4 adds strict Artifact and Evidence contracts plus Rust-owned storage.
The Trust Core allocates IDs and internal storage keys, writes only beneath
`ARTIFACT_ROOT`, computes `0x`-prefixed SHA-256 hashes over the exact stored
bytes, and appends `artifact_created` / `evidence_created` events with metadata
in the same SQLite transaction. User filenames remain display metadata and
never become storage paths. Artifact uploads and normalized Evidence JSON are
limited to `ARTIFACT_MAX_BYTES` bytes, which defaults to 1 MiB. Database
failures remove the newly written file so metadata and storage do not silently
diverge.

The Dashboard now provides a responsive, bilingual Trust Control Desk built
with Next.js, Tailwind CSS, TanStack Query, React Flow, and Lucide. It reads
authoritative health, Run, and append-only Event data through read-only
same-origin proxy routes. Audit results, Evidence Board, scores, commitments,
execution graph, and provenance views are explicitly labeled Preview because
their final Rust-backed contracts are not implemented yet. The Dashboard does
not calculate scores or Hashes and exposes no approval or chain-write action.

The minimal Foundry contract groups commitments by `toolId -> runId` and
records a Passport Hash, Audit Log Hash, and Evidence Manifest Hash. Stage 5
builds a deterministic JCS + SHA-256 hash chain over append-only Run Events.
Stage 6 has published strict shared contracts for untrusted finding
submissions, Rust-owned deterministic check results, frozen Evidence Boards
and Manifests, Passport v0.2, Provenance, and an independent Attestation
Receipt. The Rust scoring core loads the versioned `0.3.0` Standard/Profile
catalog, rejects incomplete or unversioned findings and cross-Run Evidence
references, and deterministically computes rule points, dimension scores,
total score, and high-risk-capped rating. New Runs freeze their `0.3.0` audit
catalog binding. Rust freezes normalized Evidence Boards and canonical
Manifests from persisted same-Run Evidence, saves them immutably with a
Trust-Core-owned `evidence_board_frozen` event, and requires that scoring
reference an existing frozen Board. The check-result API saves immutable
results and appends `score_changed` in one transaction. Because a trusted human
approval API is not implemented yet, approval-required `not_applicable`
findings remain closed. Passport freeze and commitment Hashes, the orchestrator
subprocess, SSE, approval records, and onchain writes are not implemented yet.

## Docker

The root `Dockerfile` builds separate non-root runtime images for the Trust
Core and Dashboard. It does not include `.env`, local databases, artifacts,
dependency directories, the orchestrator mock, or Foundry tooling.

Build and run the default Trust Core image:

```bash
docker build --target runtime -t alethos-toolpassport-backend .
docker run --rm -p 8080:8080 \
  -v alethos-data:/app/data \
  -v alethos-runs:/app/runs \
  alethos-toolpassport-backend
```

Build the Dashboard image separately:

```bash
docker build --target dashboard-runtime -t alethos-toolpassport-dashboard .
docker run --rm -p 3000:3000 \
  -e NEXT_PUBLIC_BACKEND_URL=http://host.docker.internal:8080 \
  alethos-toolpassport-dashboard
```

On Linux, connect the Dashboard container to the backend through a user-defined
Docker network and set `NEXT_PUBLIC_BACKEND_URL` to the backend container URL.
The Dockerfile does not run chain operations or deploy contracts.

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

## Application Deployment

`.github/workflows/deploy.yml` deploys the application to an existing remote
Git checkout over SSH. It checks out the requested ref in detached-HEAD mode
and then runs `scripts/deploy.sh`, which checks and builds that exact revision
before restarting the configured backend systemd service and Dashboard PM2
process. The deploy script does not pull code, deploy contracts, sign, or write
onchain.

Configure the protected `aliyun-ecs` GitHub environment with these secrets:

- `DEPLOY_HOST`: remote SSH host
- `DEPLOY_USER`: remote SSH user
- `DEPLOY_KEY`: remote SSH private key
- `DEPLOY_PATH`: absolute path of the existing remote Git checkout

The workflow can always be started manually with a branch, tag, or commit. To
also trigger it after a successful `main` CI run, set the repository variable
`AUTO_DEPLOY` to `true`. Keep required reviewers on the `aliyun-ecs`
environment when application releases need explicit authorization. This
workflow never performs wallet signing, contract deployment, or onchain writes;
those operations require a separate protected workflow and explicit human
approval.
