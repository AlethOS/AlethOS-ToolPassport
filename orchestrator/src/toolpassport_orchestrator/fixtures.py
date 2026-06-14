"""Offline mock fixtures for Stage 3 investigation mock.

All data is loaded from local profile/standard JSON files or generated
deterministically. No network requests, no GLM calls.
"""

from __future__ import annotations

import json
import pathlib
from typing import Any

from .state import EvidenceEntry, GapEntry

# Resolve repository root relative to this file location:
# src/toolpassport_orchestrator/fixtures.py → ../../../
_REPO_ROOT = pathlib.Path(__file__).parents[3]


def _load_json(rel_path: str) -> dict[str, Any]:
    path = _REPO_ROOT / rel_path
    with path.open(encoding="utf-8") as f:
        return json.load(f)  # type: ignore[no-any-return]


# ---------------------------------------------------------------------------
# Mock tool
# ---------------------------------------------------------------------------
MOCK_TOOL: dict[str, Any] = {
    "tool_id": "github:langchain-ai/langgraph",
    "name": "LangGraph",
    "tool_type": "agent_framework",
    "canonical_url": "https://github.com/langchain-ai/langgraph",
    "target_revision": "unresolved",
}

# ---------------------------------------------------------------------------
# Loaded offline profile / standard
# ---------------------------------------------------------------------------

def load_profile(profile_id: str, version: str = "0.2.0") -> dict[str, Any]:
    """Load a profile fixture from the profiles/ directory."""
    return _load_json(f"profiles/{profile_id}/{version}.json")


def load_standard(standard_id: str, version: str = "0.2.0") -> dict[str, Any]:
    """Load a standard fixture from the standards/ directory."""
    return _load_json(f"standards/{standard_id}/{version}.json")


# ---------------------------------------------------------------------------
# Evidence factory
# ---------------------------------------------------------------------------

_SOURCE_TYPES = ["official_docs", "github_readme", "public_example"]


def make_mock_evidence(check_ids: list[str], round_num: int) -> list[EvidenceEntry]:
    """Generate 2 mock evidence entries per investigation round.

    Evidence is seeded by round so each round adds distinct items.
    check_ids are distributed evenly across entries.
    """
    entries: list[EvidenceEntry] = []
    for i in range(2):
        idx = (round_num * 2 + i)
        source_type = _SOURCE_TYPES[idx % len(_SOURCE_TYPES)]
        # Distribute checks: round 0 gets first half, round 1 gets second half.
        # Within each round, evidence i=0 gets part A, i=1 gets part B.
        half = max(1, len(check_ids) // 2)
        if round_num == 0:
            round_checks = check_ids[:half]
        else:
            round_checks = check_ids[half:]
            
        r_half = max(1, len(round_checks) // 2)
        if i == 0:
            supported = round_checks[:r_half]
        else:
            supported = round_checks[r_half:]
        entries.append(
            EvidenceEntry(
                evidence_id=f"evidence-r{round_num}-{i}",
                source_type=source_type,
                source_url=f"https://example.com/mock/evidence-r{round_num}-{i}",
                title=f"Round {round_num} source {i}: {source_type}",
                excerpt=(
                    f"[mock] Documentation excerpt for round {round_num}, source {i}. "
                    f"Supports checks: {', '.join(supported) or 'none'}"
                ),
                supports=supported,
                contradicts=[],
            )
        )
    return entries


# ---------------------------------------------------------------------------
# Gap factory
# ---------------------------------------------------------------------------

def make_mock_gaps(
    profile_checks: list[dict[str, Any]],
    resolved_check_ids: set[str],
) -> list[GapEntry]:
    """Generate one GapEntry per unresolved check."""
    gaps: list[GapEntry] = []
    for check in profile_checks:
        check_id: str = check["check_id"]
        if check_id in resolved_check_ids:
            continue
        priority: str = "high" if check.get("high_risk") else "medium"
        gaps.append(
            GapEntry(
                gap_id=f"gap-{check_id}",
                check_id=check_id,
                description=f"No evidence yet for: {check.get('question', check_id)}",
                priority=priority,  # type: ignore[arg-type]
                resolved=False,
            )
        )
    return gaps
