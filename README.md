# AlethOS ToolPassport

AlethOS ToolPassport is a verifiable AI tool audit module. The repository is an
MVP monorepo with a Rust trust core, a LangGraph orchestrator, a Next.js
dashboard, and a minimal Foundry registry.

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
