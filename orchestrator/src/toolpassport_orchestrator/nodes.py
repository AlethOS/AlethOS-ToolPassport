"""Node functions for the ToolPassport investigation graph.

Each node accepts the full GraphState and returns a dict of fields to update.
Most nodes use mock/fixture data; ``hypothesis_builder_llm`` optionally calls
GLM via the LLM adapter for richer gap descriptions.

When a ``BackendClient`` is configured (via ``backend_client.set_backend_client``),
nodes that create evidence, artifacts, or events will persist them to the Rust
Trust Core. On failure they fall back to mock data.
"""

from __future__ import annotations

import json
import logging
import re
from datetime import datetime, timezone
from typing import Any, cast

from .backend_client import get_backend_client
from .fixtures import (
    load_profile,
    load_standard,
    make_mock_evidence,
    make_mock_gaps,
)
from .research import ResearchResult, Researcher
from .llm import LLMConfig, LLMError, chat_structured_list
from .state import CheckFinding, EvidenceEntry, GapEntry, GraphState, ResearchBudget

logger = logging.getLogger(__name__)


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
    """Generate initial Gap entries for every profile check (mock)."""
    checks = _profile_checks(state)
    gaps = make_mock_gaps(checks, resolved_check_ids=set())
    return {
        "current_node": "hypothesis_builder",
        "open_gaps": gaps,
    }


def hypothesis_builder_llm(state: GraphState) -> dict[str, Any]:
    """Generate Gap entries using GLM; falls back to mock on failure."""
    checks = _profile_checks(state)
    config = LLMConfig()

    if not config.is_configured:
        logger.warning("LLM not configured, falling back to mock gaps")
        return hypothesis_builder(state)

    # Build a structured prompt from the profile checks
    checks_desc = json.dumps(
        [
            {
                "check_id": c["check_id"],
                "dimension": c.get("dimension", ""),
                "question": c.get("question", ""),
                "high_risk": c.get("high_risk", False),
                "required_evidence_types": c.get("required_evidence_types", []),
            }
            for c in checks
        ],
        ensure_ascii=False,
        indent=2,
    )

    tool_ctx = ""
    if state.tool_name:
        tool_ctx += f"Tool: {state.tool_name}"
    if state.tool_type:
        tool_ctx += f" (type: {state.tool_type})"
    if state.goal:
        tool_ctx += f"\nAudit goal: {state.goal}"
    if state.audit_directives:
        tool_ctx += f"\nAudit directives: {state.audit_directives}"

    system_prompt = (
        "You are a security audit gap analyst. Given a list of audit checks "
        "for a tool, generate a precise gap description for each check. "
        "Each gap should explain what specific evidence or information needs "
        "to be gathered to evaluate that check. Be concise but specific."
    )
    user_prompt = (
        f"{tool_ctx}\n\n"
        f"Audit checks to analyze:\n{checks_desc}\n\n"
        "For each check, produce a GapEntry with:\n"
        '- gap_id: "gap-{check_id}"\n'
        "- check_id: the check's check_id\n"
        "- description: a specific, actionable description of what evidence "
        "is needed for this check (2-3 sentences)\n"
        "- priority: \"high\" if high_risk is true, otherwise \"medium\"\n"
        "- resolved: false"
    )

    try:
        gaps = chat_structured_list(
            config,
            system_prompt,
            user_prompt,
            GapEntry,
            temperature=0.3,
        )
        logger.info("GLM generated %d gap entries", len(gaps))
    except LLMError as exc:
        logger.warning("GLM call failed (%s), falling back to mock gaps", exc)
        gaps = make_mock_gaps(checks, resolved_check_ids=set())

    return {
        "current_node": "hypothesis_builder",
        "open_gaps": gaps,
    }


# ---------------------------------------------------------------------------
# Stage 3: Investigation Loop
# ---------------------------------------------------------------------------


def investigation_round(state: GraphState) -> dict[str, Any]:
    """Run one investigation round: collect evidence from real web sources.

    When the tool has a GitHub canonical URL, the researcher fetches the
    repo page and README; for other HTTPS URLs it fetches the URL directly.
    Fetched content is turned into unlinked evidence entries. Live mode never
    falls back to mock evidence.
    """
    round_num = state.research_round
    checks = _profile_checks(state)
    all_check_ids = [c["check_id"] for c in checks]
    now_iso = datetime.now(timezone.utc).isoformat()

    # ── Try real network research (explicit mode) ──────────────────
    live_research = state.research_mode == "live"
    researcher: Researcher | None = None
    real_sources: list[EvidenceEntry] = []
    research_errors: list[str] = []
    if live_research:
        try:
            researcher = Researcher()
            real_sources = _fetch_real_sources(
                researcher, state, round_num, now_iso
            )
        except Exception as exc:
            logger.warning("Real research failed: %s", exc)
            research_errors.append(f"live_research_failed:{exc}")
        finally:
            if researcher is not None:
                try:
                    researcher.close()
                except Exception:
                    pass

    # Live research never falls back to mock evidence. A failed or empty live
    # round remains visible as insufficient evidence.
    if live_research:
        base_evidence = real_sources
        if not base_evidence and not research_errors:
            research_errors.append("live_research_no_sources")
    else:
        base_evidence = make_mock_evidence(all_check_ids, round_num)

    remaining_sources = max(
        0,
        state.research_budget.max_sources - state.research_budget.sources_used,
    )
    base_evidence = base_evidence[:remaining_sources]

    if base_evidence and live_research:
        logger.info("Investigation round %d: using %d real source(s)", round_num, len(base_evidence))
    elif live_research:
        logger.info("Investigation round %d: no real sources collected", round_num)
    else:
        logger.info("Investigation round %d: using mock evidence", round_num)

    backend = get_backend_client()
    persisted_evidence: list[EvidenceEntry] = []

    for ev in base_evidence:
        if backend is None:
            persisted_evidence.append(ev)
            continue

        artifact_id: str | None = None
        snippet = ev.excerpt.encode("utf-8")
        artifact_result = backend.upload_artifact(
            state.run_id,
            f"{ev.evidence_id}.txt",
            snippet,
            "text/plain; charset=utf-8",
        )
        if artifact_result is not None:
            artifact_id = artifact_result.get("artifact_id")

        evidence_payload: dict[str, Any] = {
            "evidence_schema_version": "0.2.0",
            "source_type": ev.source_type,
            "source_url": ev.source_url,
            "source_revision": None,
            "title": ev.title,
            "excerpt": ev.excerpt,
            "retrieved_at": now_iso,
            "snapshot_artifact_id": artifact_id,
            "supports": ev.supports,
            "contradicts": ev.contradicts,
            "metadata": {},
        }

        result = backend.create_evidence(state.run_id, evidence_payload)
        if result is not None:
            persisted_evidence.append(EvidenceEntry.from_backend(result))
        elif live_research:
            research_errors.append(f"live_evidence_persistence_failed:{ev.source_url}")
        else:
            logger.debug("Mock evidence persistence failed for %s, using local", ev.evidence_id)
            persisted_evidence.append(ev)

    # Determine which gaps are now resolved (those supported by new evidence)
    newly_supported: set[str] = set()
    for ev in persisted_evidence:
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
        sources_used=state.research_budget.sources_used + len(persisted_evidence),
    )

    return {
        "current_node": "investigation_round",
        "research_round": round_num + 1,
        "research_budget": budget,
        "evidence_board": list(state.evidence_board) + persisted_evidence,
        "open_gaps": updated_gaps,
        "errors": list(state.errors) + research_errors,
    }


def _fetch_real_sources(
    researcher: Researcher,
    state: GraphState,
    round_num: int,
    now_iso: str,
) -> list[EvidenceEntry]:
    """Fetch real web sources for the investigation round.

    Returns a list of EvidenceEntry objects built from fetched pages.
    Returns an empty list if no sources could be fetched.
    """
    entries: list[EvidenceEntry] = []

    target_url = state.canonical_url or ""
    tool_name = state.tool_name or state.tool_id or "unknown"

    # Parse GitHub owner/repo from URL.
    gh_match = re.match(r"https?://github\.com/([^/]+)/([^/]+?)(?:\.git)?/?$", target_url)
    if gh_match:
        owner, repo = gh_match.group(1), gh_match.group(2)
        result = researcher.fetch_github_repo(owner, repo)
    else:
        # Generic URL fetch.
        result = researcher.fetch_urls([target_url]) if target_url.startswith("https://") else \
            ResearchResult()

    if not result.pages:
        logger.info("No real pages fetched; will fall back to mock")
        return []

    for idx, page in enumerate(result.pages):
        summary = researcher.extract_summary(page, max_chars=2_000)
        if not summary.strip():
            continue

        eid = f"real-r{round_num}-{idx}"
        source_type = "github_readme" if "raw.githubusercontent.com" in page.url else "official_docs"
        title = f"Real source: {tool_name} (round {round_num}, page {idx})"

        entries.append(
            EvidenceEntry(
                evidence_id=eid,
                source_type=source_type,
                source_url=page.url,
                title=title,
                excerpt=summary,
                retrieved_at=now_iso,
                # Collection alone does not prove a check. A later validated
                # mapping step must explicitly link this evidence.
                supports=[],
                contradicts=[],
            )
        )

    if result.errors:
        logger.warning("Research errors: %s", result.errors)

    return entries


def gap_analysis(state: GraphState) -> dict[str, Any]:
    """Analyze open gaps to decide whether to continue research or freeze."""
    open_count = sum(1 for g in state.open_gaps if not g.resolved)
    total_count = len(state.open_gaps)
    round_budget_exhausted = state.research_round >= state.research_budget.max_rounds
    source_budget_exhausted = (
        state.research_budget.sources_used >= state.research_budget.max_sources
    )

    if round_budget_exhausted:
        stop_reason = f"max_rounds_reached ({state.research_budget.max_rounds})"
    elif source_budget_exhausted:
        stop_reason = f"max_sources_reached ({state.research_budget.max_sources})"
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
    """Freeze the evidence board and transition to evaluation phase.

    When a BackendClient is configured, submits the freeze proposal to the
    Rust Trust Core and records the returned board version.
    """
    updates: dict[str, Any] = {
        "current_node": "freeze_evidence_board",
        "phase": "evaluation",
    }

    backend = get_backend_client()
    if backend is None:
        return updates

    checks = _profile_checks(state)
    valid_check_ids = {c["check_id"] for c in checks}

    # Build gaps from open_gaps state
    gap_entries: list[dict[str, Any]] = []
    for g in state.open_gaps:
        if g.check_id not in valid_check_ids:
            continue
        status = "resolved" if g.resolved else "open"
        gap_entries.append({
            "gap_id": g.gap_id,
            "check_id": g.check_id,
            "description": g.description,
            "priority": g.priority,
            "status": status,
            "resolution": None,
        })

    # Build claims from evidence board
    claim_entries: list[dict[str, Any]] = []
    for idx, ev in enumerate(state.evidence_board):
        if len(ev.supports) > 0:
            claim_entries.append({
                "claim_id": f"claim-e{idx}",
                "check_id": ev.supports[0],
                "statement": f"Evidence from {ev.title}",
                "status": "supported",
                "confidence": 0.7,
                "supports": ev.supports,
                "contradicts": ev.contradicts,
            })

    evidence_ids = [ev.evidence_id for ev in state.evidence_board]

    request: dict[str, Any] = {
        "evidence_board_schema_version": "0.1.0",
        "version": 1,
        "evidence_ids": evidence_ids,
        "claims": claim_entries,
        "gaps": gap_entries,
        "freeze_reason": state.stop_reason or "investigation complete",
    }

    result = backend.freeze_evidence_board(state.run_id, request)
    if result is not None:
        board = result.get("evidence_board", {})
        updates["frozen_board"] = {
            "version": board.get("version", 1),
            "frozen_at": board.get("frozen_at", ""),
        }

    return updates


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

    # Optionally submit to backend.
    updates: dict[str, Any] = {
        "current_node": "check_execution",
        "check_findings": findings,
    }

    backend = get_backend_client()
    if backend is None or state.frozen_board is None:
        return updates

    finding_submissions: list[dict[str, Any]] = []
    for f in findings:
        submission: dict[str, Any] = {
            "check_id": f.check_id,
            "finding": f.finding,
            "rationale": f.rationale,
            "evidence_ids": f.evidence_ids,
            "not_applicable_reason": None,
        }
        finding_submissions.append(submission)

    check_submission: dict[str, Any] = {
        "check_results_schema_version": "0.1.0",
        "evidence_board_version": state.frozen_board.version,
        "findings": finding_submissions,
    }

    result = backend.submit_check_results(state.run_id, check_submission)
    if result is not None:
        updates["check_results_ref"] = {
            "check_results_id": result.get("check_results_id", ""),
            "evidence_board_version": result.get("evidence_board_version", 1),
            "total_score": result.get("total_score", 0),
            "rating": result.get("rating", ""),
        }

    return updates


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
    updates: dict[str, Any] = {
        "current_node": "passport_draft",
        "phase": "done",
        "passport_draft": draft,
    }

    backend = get_backend_client()
    if backend is None or state.frozen_board is None:
        return updates

    # Build a minimal FreezePassportRequest from the draft data.
    passport_request: dict[str, Any] = {
        "passport_version": "0.2.0",
        "evidence_board_version": state.frozen_board.version,
        "target_revision": state.target_revision,
        "audit_scope": state.goal,
        "capability_claims": [
            {
                "statement_id": f"cap-{f.check_id}",
                "statement": f.rationale,
                "status": "supported",
                "evidence_ids": f.evidence_ids,
            }
            for f in state.check_findings
            if f.finding in ("pass", "partial")
        ],
        "interfaces": [],
        "risks": [],
        "known_gaps": [
            g.description for g in state.open_gaps if not g.resolved
        ],
        "recommendation": {
            "summary": f"Audit completed with stop reason: {state.stop_reason}. "
            f"Evidence collected: {len(state.evidence_board)} items.",
            "conditions": [],
        },
    }

    result = backend.freeze_passport(state.run_id, passport_request)
    if result is not None:
        prov = result.get("provenance", {})
        updates["passport_sequence"] = prov.get("passport_sequence")

    return updates
