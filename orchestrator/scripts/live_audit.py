#!/usr/bin/env python
"""Live audit demo — fetches real web sources for a GitHub project.

Usage:
    ORCHESTRATOR_LIVE_RESEARCH=true python scripts/live_audit.py https://github.com/owner/repo

Set BACKEND_URL to persist evidence to a running Rust Trust Core.
"""

from __future__ import annotations

import json
import os
import re
import sys
from typing import Any

from toolpassport_orchestrator import GraphState, build_graph
from toolpassport_orchestrator.backend_client import BackendClient, set_backend_client
from toolpassport_orchestrator.state import ResearchBudget


def parse_github_url(url: str) -> tuple[str, str] | None:
    """Extract (owner, repo) from a GitHub URL."""
    m = re.match(r"https?://github\.com/([^/]+)/([^/]+?)(?:\.git)?/?$", url)
    if m:
        return m.group(1), m.group(2)
    return None


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: python scripts/live_audit.py <github-url>")
        print("Example: python scripts/live_audit.py https://github.com/langchain-ai/langgraph")
        sys.exit(1)

    target_url = sys.argv[1].rstrip("/")
    parsed = parse_github_url(target_url)
    if not parsed:
        print(f"Error: {target_url} is not a valid GitHub repo URL")
        sys.exit(1)

    owner, repo = parsed
    tool_id = f"github:{owner}/{repo}"
    print(f"Auditing: {tool_id} ({target_url})")
    print()

    # Use RUN_ID from environment if set (launched by backend).
    run_id = os.environ.get("RUN_ID", "00000000-0000-0000-0000-000000000001")

    # Configure backend if available.
    backend_url = os.environ.get("BACKEND_URL", "")
    backend = None
    if backend_url:
        backend = BackendClient(base_url=backend_url)
        set_backend_client(backend)
        print(f"Backend: {backend_url}")

    initial = GraphState(
        run_id=run_id,
        goal=f"Audit {owner}/{repo} as a software tool",
        research_mode="live",
        tool_id=tool_id,
        tool_name=repo,
        tool_type="generic",
        canonical_url=target_url,
        target_revision="main",
        research_budget=ResearchBudget(max_rounds=3, max_sources=30),
    )

    print(f"max_rounds: {initial.research_budget.max_rounds}")
    print()

    graph = build_graph()
    result = GraphState.model_validate(graph.invoke(initial))

    print(f"phase:         {result.phase}")
    print(f"research_rounds: {result.research_round}")
    print(f"stop_reason:   {result.stop_reason}")
    print(f"evidence_count: {len(result.evidence_board)}")
    print()

    print("=== Evidence ===")
    for ev in result.evidence_board:
        print(f"  [{ev.source_type}] {ev.evidence_id}")
        print(f"    url:  {ev.source_url}")
        print(f"    excerpt: {ev.excerpt[:120]}...")
        print()

    print("=== Check Findings ===")
    for f in result.check_findings:
        print(f"  [{f.finding:8s}] {f.check_id}  (evidence: {len(f.evidence_ids)})")

    if result.passport_draft:
        draft: dict[str, Any] = result.passport_draft
        print()
        print("=== Passport Draft ===")
        safe = {k: v for k, v in draft.items() if k not in ("check_findings", "evidence_board")}
        print(json.dumps(safe, indent=2))

    if backend:
        backend.close()
        set_backend_client(None)


if __name__ == "__main__":
    main()
