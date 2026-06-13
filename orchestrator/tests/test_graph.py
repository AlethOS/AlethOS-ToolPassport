"""Tests for the Stage 3 investigation mock graph."""

from __future__ import annotations

import pytest

from toolpassport_orchestrator import GraphState, build_graph
from toolpassport_orchestrator.fixtures import MOCK_TOOL, load_profile
from toolpassport_orchestrator.state import CheckFinding, ResearchBudget


def _make_initial(**overrides: object) -> GraphState:
    defaults: dict[str, object] = {
        "run_id": "00000000-0000-0000-0000-000000000001",
        "goal": "Audit a mock AI tool",
        "tool_id": MOCK_TOOL["tool_id"],
        "tool_name": MOCK_TOOL["name"],
        "tool_type": MOCK_TOOL["tool_type"],
        "target_revision": MOCK_TOOL["target_revision"],
    }
    defaults.update(overrides)
    return GraphState(**defaults)  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# Backward-compat: confirm a basic run still reaches a node near plan_audit
# ---------------------------------------------------------------------------


def test_mock_graph_reaches_plan_audit() -> None:
    """Original acceptance: graph completes without error."""
    result = GraphState.model_validate(build_graph().invoke(_make_initial()))
    # Stage 3 graph ends at passport_draft, not plan_audit — accept any completion
    assert result.current_node != ""
    assert result.phase in {"investigation", "evaluation", "done"}


# ---------------------------------------------------------------------------
# New Stage 3 acceptance tests
# ---------------------------------------------------------------------------


def test_full_investigation_completes() -> None:
    """Graph completes two+ rounds, produces findings for every check and a passport draft."""
    result = GraphState.model_validate(build_graph().invoke(_make_initial()))

    assert result.phase == "done", f"expected done, got {result.phase}"
    assert result.research_round >= 2, f"expected ≥2 rounds, got {result.research_round}"

    # All profile checks should have a finding
    profile = load_profile(result.profile_id or "generic", result.profile_version or "0.2.0")
    expected_check_ids = {c["check_id"] for c in profile["checks"]}
    actual_check_ids = {f.check_id for f in result.check_findings}
    assert expected_check_ids == actual_check_ids, (
        f"Missing findings for: {expected_check_ids - actual_check_ids}"
    )

    assert result.passport_draft is not None
    assert result.passport_draft["tool_id"] == MOCK_TOOL["tool_id"]
    assert result.passport_draft["profile_id"] == result.profile_id


def test_stop_condition_respects_max_rounds() -> None:
    """With max_rounds=1, investigation stops after exactly 1 round."""
    initial = _make_initial(research_budget=ResearchBudget(max_rounds=1))
    result = GraphState.model_validate(build_graph().invoke(initial))

    assert result.research_round == 1, f"expected 1 round, got {result.research_round}"
    assert result.stop_reason is not None
    assert "max_rounds_reached" in result.stop_reason
    assert result.phase == "done"
    assert result.passport_draft is not None


def test_skeptic_review_downgrades_weak_high_risk() -> None:
    """Skeptic Review must downgrade a high-risk 'pass' backed by only 1 evidence to 'partial'."""
    profile = load_profile("agent_framework", "0.2.0")
    high_risk_checks = [c for c in profile["checks"] if c.get("high_risk")]
    assert high_risk_checks, "agent_framework profile must have at least one high_risk check"
    target_check_id = high_risk_checks[0]["check_id"]

    # Inject a pre-built state that is already at check_execution stage but with
    # only 1 evidence for the high_risk check — then run skeptic_review in isolation.
    from toolpassport_orchestrator.nodes import skeptic_review

    weak_finding = CheckFinding(
        check_id=target_check_id,
        finding="pass",
        rationale="[test] only one evidence item",
        evidence_ids=["ev-001"],  # only 1 → should be downgraded
    )
    pre_state = GraphState(
        run_id="00000000-0000-0000-0000-000000000002",
        goal="Test skeptic review downgrade",
        tool_id=MOCK_TOOL["tool_id"],
        tool_name=MOCK_TOOL["name"],
        tool_type=MOCK_TOOL["tool_type"],
        profile_id="agent_framework",
        profile_version="0.2.0",
        check_findings=[weak_finding],
    )

    updates = skeptic_review(pre_state)
    updated_findings: list[CheckFinding] = updates["check_findings"]
    target = next(f for f in updated_findings if f.check_id == target_check_id)

    assert target.finding == "partial", (
        f"Expected 'partial' after skeptic review, got '{target.finding}'"
    )
    assert any("high_risk_downgrade" in issue for issue in updates["review_issues"])


def test_intake_rejects_empty_goal() -> None:
    """intake_normalization must raise ValueError for an empty goal."""
    from toolpassport_orchestrator.nodes import intake_normalization

    bad_state = GraphState(
        run_id="00000000-0000-0000-0000-000000000003",
        goal="   ",  # whitespace only
        tool_id=MOCK_TOOL["tool_id"],
    )
    with pytest.raises(ValueError, match="goal must not be empty"):
        intake_normalization(bad_state)
