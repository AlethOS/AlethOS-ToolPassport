"""Node functions for the ToolPassport investigation mock graph.

Each node accepts the full GraphState and returns a dict of fields to update.
All logic uses mock/fixture data — no GLM, network, or database calls.
"""

from __future__ import annotations

from typing import Any, cast

from .fixtures import (
    load_profile,
    load_standard,
    make_mock_evidence,
    make_mock_gaps,
)
from .state import CheckFinding, GapEntry, GraphState, ResearchBudget


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _profile_checks(state: GraphState) -> list[dict[str, Any]]:
    profile_id = state.profile_id or "generic"
    profile_version = state.profile_version or "0.2.0"
    profile = load_profile(profile_id, profile_version)
    return cast(list[dict[str, Any]], profile.get("checks", []))


# ---------------------------------------------------------------------------
# Stage 0: Intake
# ---------------------------------------------------------------------------


def intake_normalization(state: GraphState) -> dict[str, Any]:
    """Validate goal and tool ID; extract directive constraints."""
    if not state.goal.strip():
        raise ValueError("goal must not be empty")
    if state.tool_id is None:
        raise ValueError("tool_id must be set before starting investigation")

    # Simple directive extraction: note focus dimensions mentioned
    focus: list[str] = []
    directives = state.audit_directives or ""
    for kw in ["permission", "risk", "portability", "interface", "capability", "evidence"]:
        if kw in directives.lower():
            focus.append(kw)

    return {
        "current_node": "intake_normalization",
        "phase": "intake",
        "audit_directives": (
            f"{directives} [focus:{','.join(focus)}]" if focus else directives or None
        ),
    }


# ---------------------------------------------------------------------------
# Stage 1: Fingerprint & Profile Selection
# ---------------------------------------------------------------------------


def tool_fingerprint(state: GraphState) -> dict[str, Any]:
    """Determine tool type candidate from tool metadata (mock: confidence=0.9)."""
    return {
        "current_node": "tool_fingerprint",
        "tool_type": state.tool_type,  # already set from MOCK_TOOL
    }


def profile_selector(state: GraphState) -> dict[str, Any]:
    """Select profile based on tool_type and confidence threshold."""
    tool_type = state.tool_type or "unknown"
    # Known specialized profiles
    specialized = {"agent_framework", "mcp_server", "cli_api_tool"}
    confidence = 0.9 if tool_type in specialized else 0.5

    if confidence >= 0.75 and tool_type in specialized:
        profile_id = tool_type
    else:
        profile_id = "generic"

    return {
        "current_node": "profile_selector",
        "profile_id": profile_id,
        "profile_version": "0.2.0",
        "standard_version": "0.2.0",
    }


# ---------------------------------------------------------------------------
# Stage 2: Planning & Hypotheses
# ---------------------------------------------------------------------------


def audit_plan_builder(state: GraphState) -> dict[str, Any]:
    """Build ordered audit plan from profile checks (high_risk first, then by weight desc)."""
    checks = _profile_checks(state)
    # Sort: high_risk first, then by weight descending
    ordered = sorted(checks, key=lambda c: (not c.get("high_risk", False), -c.get("weight", 1)))
    _ = ordered  # plan is carried implicitly via profile; no extra state field needed
    _ = load_standard("alethos-toolpassport", state.standard_version)
    return {
        "current_node": "audit_plan_builder",
        "phase": "investigation",
    }


def hypothesis_builder(state: GraphState) -> dict[str, Any]:
    """Generate initial Gap entries for every profile check."""
    checks = _profile_checks(state)
    gaps = make_mock_gaps(checks, resolved_check_ids=set())
    return {
        "current_node": "hypothesis_builder",
        "open_gaps": gaps,
    }


# ---------------------------------------------------------------------------
# Stage 3: Investigation Loop
# ---------------------------------------------------------------------------


def investigation_round(state: GraphState) -> dict[str, Any]:
    """Run one investigation round: collect mock evidence, close some gaps."""
    round_num = state.research_round
    checks = _profile_checks(state)
    all_check_ids = [c["check_id"] for c in checks]

    # Generate 2 new evidence entries this round
    new_evidence = make_mock_evidence(all_check_ids, round_num)

    # Determine which gaps are now resolved (those supported by new evidence)
    newly_supported: set[str] = set()
    for ev in new_evidence:
        newly_supported.update(ev.supports)

    updated_gaps = [
        GapEntry(
            gap_id=g.gap_id,
            check_id=g.check_id,
            description=g.description,
            priority=g.priority,
            resolved=g.resolved or g.check_id in newly_supported,
        )
        for g in state.open_gaps
    ]

    budget = ResearchBudget(
        max_rounds=state.research_budget.max_rounds,
        max_sources=state.research_budget.max_sources,
        sources_used=state.research_budget.sources_used + len(new_evidence),
    )

    return {
        "current_node": "investigation_round",
        "research_round": round_num + 1,
        "research_budget": budget,
        "evidence_board": list(state.evidence_board) + new_evidence,
        "open_gaps": updated_gaps,
    }


def gap_analysis(state: GraphState) -> dict[str, Any]:
    """Analyze open gaps to decide whether to continue research or freeze."""
    open_count = sum(1 for g in state.open_gaps if not g.resolved)
    total_count = len(state.open_gaps)
    budget_exhausted = state.research_round >= state.research_budget.max_rounds

    if budget_exhausted:
        stop_reason = f"max_rounds_reached ({state.research_budget.max_rounds})"
    elif open_count == 0:
        stop_reason = "all_gaps_resolved"
    elif open_count <= max(1, total_count // 4):
        # Fewer than 25 % gaps remain — stop; record remaining as insufficient_evidence
        stop_reason = f"sufficient_coverage ({open_count} gaps remain)"
    else:
        stop_reason = None  # continue

    return {
        "current_node": "gap_analysis",
        "stop_reason": stop_reason,
    }


# ---------------------------------------------------------------------------
# Stage 4: Freeze & Evaluation
# ---------------------------------------------------------------------------


def freeze_evidence_board(state: GraphState) -> dict[str, Any]:
    """Freeze the evidence board and transition to evaluation phase."""
    return {
        "current_node": "freeze_evidence_board",
        "phase": "evaluation",
    }


def check_execution(state: GraphState) -> dict[str, Any]:
    """Produce a CheckFinding for each profile check based on available evidence."""
    checks = _profile_checks(state)

    # Build index: check_id → list of supporting evidence IDs
    support_index: dict[str, list[str]] = {}
    for ev in state.evidence_board:
        for cid in ev.supports:
            support_index.setdefault(cid, []).append(ev.evidence_id)

    findings: list[CheckFinding] = []
    for check in checks:
        cid = check["check_id"]
        evidence_ids = support_index.get(cid, [])
        if len(evidence_ids) >= 2:
            finding: str = "pass"
            rationale = (
                f"[mock] {len(evidence_ids)} evidence items support this check."
            )
        elif len(evidence_ids) == 1:
            finding = "partial"
            rationale = "[mock] Only one evidence item found; partial coverage."
        else:
            finding = "unknown"
            rationale = "[mock] No evidence collected for this check."
        findings.append(
            CheckFinding(
                check_id=cid,
                finding=finding,  # type: ignore[arg-type]
                rationale=rationale,
                evidence_ids=evidence_ids,
            )
        )

    return {
        "current_node": "check_execution",
        "check_findings": findings,
    }


# ---------------------------------------------------------------------------
# Stage 5: Skeptic Review
# ---------------------------------------------------------------------------


def skeptic_review(state: GraphState) -> dict[str, Any]:
    """Downgrade weak high-risk findings; record review issues."""
    checks = _profile_checks(state)
    high_risk_ids = {c["check_id"] for c in checks if c.get("high_risk")}

    updated_findings: list[CheckFinding] = []
    issues: list[str] = list(state.review_issues)

    for f in state.check_findings:
        if f.check_id in high_risk_ids and f.finding == "pass" and len(f.evidence_ids) < 2:
            # Downgrade: high-risk pass requires at least 2 supporting evidence items
            updated_findings.append(
                CheckFinding(
                    check_id=f.check_id,
                    finding="partial",
                    rationale=(
                        f"[skeptic] Downgraded from pass to partial: high-risk check "
                        f"requires ≥2 evidence items, found {len(f.evidence_ids)}."
                    ),
                    evidence_ids=f.evidence_ids,
                )
            )
            issues.append(
                f"high_risk_downgrade:{f.check_id} (evidence_count={len(f.evidence_ids)})"
            )
        else:
            updated_findings.append(f)

    return {
        "current_node": "skeptic_review",
        "check_findings": updated_findings,
        "review_issues": issues,
    }


# ---------------------------------------------------------------------------
# Stage 6: Passport Draft
# ---------------------------------------------------------------------------


def passport_draft(state: GraphState) -> dict[str, Any]:
    """Assemble a passport draft dict from frozen evidence board and findings."""
    findings_data = [
        {
            "check_id": f.check_id,
            "finding": f.finding,
            "rationale": f.rationale,
            "evidence_ids": f.evidence_ids,
        }
        for f in state.check_findings
    ]
    evidence_data = [
        {
            "evidence_id": e.evidence_id,
            "source_type": e.source_type,
            "title": e.title,
            "excerpt": e.excerpt,
            "supports": e.supports,
        }
        for e in state.evidence_board
    ]
    draft: dict[str, Any] = {
        "run_id": state.run_id,
        "tool_id": state.tool_id,
        "tool_name": state.tool_name,
        "tool_type": state.tool_type,
        "target_revision": state.target_revision,
        "standard_id": "alethos-toolpassport",
        "standard_version": state.standard_version,
        "profile_id": state.profile_id,
        "profile_version": state.profile_version,
        "research_rounds_completed": state.research_round,
        "stop_reason": state.stop_reason,
        "evidence_count": len(state.evidence_board),
        "open_gap_count": sum(1 for g in state.open_gaps if not g.resolved),
        "review_issues": state.review_issues,
        "check_findings": findings_data,
        "evidence_board": evidence_data,
        "_note": "mock draft — not scored by Rust; deterministic scoring is Stage 6",
    }
    return {
        "current_node": "passport_draft",
        "phase": "done",
        "passport_draft": draft,
    }
