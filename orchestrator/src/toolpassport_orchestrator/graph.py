"""Stage 3 investigation graph.

Routing:
  START
    → intake_normalization
    → tool_fingerprint
    → profile_selector
    → audit_plan_builder
    → hypothesis_builder (mock or LLM)
    → investigation_round   ← loop back here when continuing
    → gap_analysis
    → (conditional) investigation_round | freeze_evidence_board
    → check_execution
    → skeptic_review
    → persist_check_results
    → passport_draft
    → END

Set ``use_llm=True`` or the env var ``ORCHESTRATOR_USE_LLM=true`` to enable
GLM-powered gap generation in the hypothesis_builder node.
"""

from __future__ import annotations

import os
from collections.abc import Callable
from functools import wraps
from typing import Any

from langgraph.graph import END, START, StateGraph

from .backend_client import get_backend_client
from .nodes import (
    audit_plan_builder,
    check_execution,
    freeze_evidence_board,
    gap_analysis,
    hypothesis_builder,
    hypothesis_builder_llm,
    intake_normalization,
    investigation_round,
    passport_draft,
    persist_check_results,
    profile_selector,
    skeptic_review,
    tool_fingerprint,
)
from .state import GraphState


def _should_continue(state: GraphState) -> str:
    """Route after gap_analysis: continue research or freeze."""
    if state.stop_reason is None:
        return "investigation_round"
    return "freeze_evidence_board"


def _append_required_event(
    state: GraphState,
    node_id: str,
    event_type: str,
    payload: dict[str, Any],
) -> None:
    backend = get_backend_client()
    if backend is None:
        return
    if backend.append_event(state.run_id, node_id, event_type, payload) is None:
        raise RuntimeError(f"failed to persist required {event_type} event for {node_id}")


def _decision_events(
    node_id: str,
    state: GraphState,
    updates: dict[str, Any],
) -> list[tuple[str, dict[str, Any]]]:
    if node_id == "intake_normalization" and updates.get("audit_directives"):
        return [("directives_accepted", {"directives": updates["audit_directives"]})]
    if node_id == "profile_selector":
        return [(
            "profile_selected",
            {
                "profile_id": updates.get("profile_id"),
                "profile_version": updates.get("profile_version"),
            },
        )]
    if node_id == "hypothesis_builder":
        return [("hypothesis_created", {"gap_count": len(updates.get("open_gaps", []))})]
    if node_id == "investigation_round":
        return [(
            "research_query_planned",
            {
                "research_round": updates.get("research_round"),
                "collected_evidence_count": len(updates.get("evidence_board", [])),
            },
        )]
    if node_id == "gap_analysis":
        return [(
            "gap_detected",
            {
                "open_gap_count": sum(1 for gap in state.open_gaps if not gap.resolved),
                "stop_reason": updates.get("stop_reason"),
            },
        )]
    if node_id == "skeptic_review" and updates.get("review_issues"):
        return [("review_issue_found", {"issues": updates["review_issues"]})]
    return []


def _instrument_node(
    node_id: str,
    node: Callable[[GraphState], dict[str, Any]],
) -> Any:
    @wraps(node)
    def instrumented(state: GraphState) -> dict[str, Any]:
        _append_required_event(
            state,
            node_id,
            "node_started",
            {"phase": state.phase, "research_round": state.research_round},
        )
        try:
            updates = node(state)
        except Exception as exc:
            _append_required_event(
                state,
                node_id,
                "error",
                {"error_type": type(exc).__name__, "message": str(exc)},
            )
            raise

        for event_type, payload in _decision_events(node_id, state, updates):
            _append_required_event(state, node_id, event_type, payload)
        _append_required_event(
            state,
            node_id,
            "node_finished",
            {"updated_fields": sorted(updates)},
        )
        return updates

    return instrumented


def build_graph(*, use_llm: bool = False, checkpointer: Any = None) -> Any:
    """Compile the investigation graph.

    When *use_llm* is true (or ``ORCHESTRATOR_USE_LLM=true`` in the
    environment), ``hypothesis_builder`` calls GLM for gap descriptions.
    Otherwise it uses deterministic mock data.

    When *checkpointer* is provided (e.g. a ``langgraph.checkpoint.sqlite.SqliteSaver``),
    the graph state is persisted after each node, enabling interrupt/resume.
    """
    env_llm = os.environ.get("ORCHESTRATOR_USE_LLM", "").lower() in ("true", "1", "yes")
    effective_llm = use_llm or env_llm

    builder: StateGraph[GraphState, None, GraphState, GraphState] = StateGraph(GraphState)

    # Register all nodes
    builder.add_node(
        "intake_normalization",
        _instrument_node("intake_normalization", intake_normalization),
    )
    builder.add_node(
        "tool_fingerprint",
        _instrument_node("tool_fingerprint", tool_fingerprint),
    )
    builder.add_node(
        "profile_selector",
        _instrument_node("profile_selector", profile_selector),
    )
    builder.add_node(
        "audit_plan_builder",
        _instrument_node("audit_plan_builder", audit_plan_builder),
    )
    builder.add_node(
        "hypothesis_builder",
        _instrument_node(
            "hypothesis_builder",
            hypothesis_builder_llm if effective_llm else hypothesis_builder,
        ),
    )
    builder.add_node(
        "investigation_round",
        _instrument_node("investigation_round", investigation_round),
    )
    builder.add_node("gap_analysis", _instrument_node("gap_analysis", gap_analysis))
    builder.add_node(
        "freeze_evidence_board",
        _instrument_node("freeze_evidence_board", freeze_evidence_board),
    )
    builder.add_node(
        "check_execution",
        _instrument_node("check_execution", check_execution),
    )
    builder.add_node(
        "skeptic_review",
        _instrument_node("skeptic_review", skeptic_review),
    )
    builder.add_node(
        "persist_check_results",
        _instrument_node("persist_check_results", persist_check_results),
    )
    builder.add_node("passport_draft", _instrument_node("passport_draft", passport_draft))

    # Linear pre-investigation path
    builder.add_edge(START, "intake_normalization")
    builder.add_edge("intake_normalization", "tool_fingerprint")
    builder.add_edge("tool_fingerprint", "profile_selector")
    builder.add_edge("profile_selector", "audit_plan_builder")
    builder.add_edge("audit_plan_builder", "hypothesis_builder")
    builder.add_edge("hypothesis_builder", "investigation_round")

    # Investigation loop
    builder.add_edge("investigation_round", "gap_analysis")
    builder.add_conditional_edges(
        "gap_analysis",
        _should_continue,
        {
            "investigation_round": "investigation_round",
            "freeze_evidence_board": "freeze_evidence_board",
        },
    )

    # Evaluation path
    builder.add_edge("freeze_evidence_board", "check_execution")
    builder.add_edge("check_execution", "skeptic_review")
    builder.add_edge("skeptic_review", "persist_check_results")
    builder.add_edge("persist_check_results", "passport_draft")
    builder.add_edge("passport_draft", END)

    return builder.compile(checkpointer=checkpointer)
