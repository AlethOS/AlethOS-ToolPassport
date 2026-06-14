"""Tests for authoritative node events and reviewed scoring order."""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any, cast

import pytest

from toolpassport_orchestrator.backend_client import BackendClient, set_backend_client
from toolpassport_orchestrator.fixtures import MOCK_TOOL
from toolpassport_orchestrator.graph import _instrument_node
from toolpassport_orchestrator.nodes import human_review_gate, persist_check_results, skeptic_review
from toolpassport_orchestrator.state import CheckFinding, FrozenBoardRef, GraphState


class FakeBackend:
    def __init__(self) -> None:
        self.events: list[tuple[str, str, dict[str, Any]]] = []
        self.submitted_findings: list[dict[str, Any]] = []

    def append_event(
        self,
        run_id: str,
        node_id: str,
        event_type: str,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        self.events.append((node_id, event_type, payload or {}))
        return {"run_id": run_id}

    def submit_check_results(
        self,
        run_id: str,
        submission: dict[str, Any],
    ) -> dict[str, Any]:
        self.submitted_findings = submission["findings"]
        return {
            "check_results_id": f"reviewed-results-{run_id}",
            "evidence_board_version": submission["evidence_board_version"],
            "total_score": 40,
            "rating": "trial",
        }


@pytest.fixture(autouse=True)
def clear_backend_client() -> Iterator[None]:
    set_backend_client(None)
    yield
    set_backend_client(None)


def _state(**overrides: Any) -> GraphState:
    values: dict[str, Any] = {
        "run_id": "00000000-0000-0000-0000-000000000020",
        "goal": "Verify eventing",
        "tool_id": MOCK_TOOL["tool_id"],
        "tool_name": MOCK_TOOL["name"],
        "tool_type": MOCK_TOOL["tool_type"],
        "canonical_url": MOCK_TOOL["canonical_url"],
        "profile_id": "agent_framework",
        "profile_version": "0.2.0",
    }
    values.update(overrides)
    return GraphState(**values)


def test_instrumented_node_persists_start_decision_and_finish_events() -> None:
    backend = FakeBackend()
    set_backend_client(cast(BackendClient, backend))

    wrapped = _instrument_node(
        "profile_selector",
        lambda _state: {
            "current_node": "profile_selector",
            "profile_id": "agent_framework",
            "profile_version": "0.2.0",
        },
    )
    wrapped(_state())

    assert [event_type for _, event_type, _ in backend.events] == [
        "node_started",
        "profile_selected",
        "node_finished",
    ]


def test_skeptic_reviewed_findings_are_submitted_for_rust_scoring() -> None:
    backend = FakeBackend()
    set_backend_client(cast(BackendClient, backend))
    weak_finding = CheckFinding(
        check_id="agent_framework.tool_permission_isolation",
        finding="pass",
        rationale="Weak pass",
        evidence_ids=["evidence-1"],
    )
    initial = _state(
        check_findings=[weak_finding],
        frozen_board=FrozenBoardRef(version=1),
    )

    reviewed = skeptic_review(initial)
    reviewed_state = initial.model_copy(update=reviewed)
    updates = persist_check_results(reviewed_state)

    submitted = next(
        finding
        for finding in backend.submitted_findings
        if finding["check_id"] == weak_finding.check_id
    )
    assert submitted["finding"] == "partial"
    assert updates["check_results_ref"]["check_results_id"].startswith("reviewed-results-")


def test_human_gate_finishes_before_requesting_approval() -> None:
    backend = FakeBackend()
    set_backend_client(cast(BackendClient, backend))

    updates = _instrument_node("human_review_gate", human_review_gate)(
        _state(passport_sequence=1)
    )

    assert updates["approval_status"] == "waiting"
    assert [event_type for _, event_type, _ in backend.events] == [
        "node_started",
        "node_finished",
        "approval_required",
    ]
