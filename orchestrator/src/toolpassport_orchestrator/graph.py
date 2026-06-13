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
    → passport_draft
    → END

Set ``use_llm=True`` or the env var ``ORCHESTRATOR_USE_LLM=true`` to enable
GLM-powered gap generation in the hypothesis_builder node.
"""

from __future__ import annotations

import os
from typing import Any

from langgraph.graph import END, START, StateGraph

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


def build_graph(*, use_llm: bool = False) -> Any:  # CompiledStateGraph[GraphState, ...]
    """Compile the investigation graph.

    When *use_llm* is true (or ``ORCHESTRATOR_USE_LLM=true`` in the
    environment), ``hypothesis_builder`` calls GLM for gap descriptions.
    Otherwise it uses deterministic mock data.
    """
    env_llm = os.environ.get("ORCHESTRATOR_USE_LLM", "").lower() in ("true", "1", "yes")
    effective_llm = use_llm or env_llm

    builder: StateGraph[GraphState, None, GraphState, GraphState] = StateGraph(GraphState)

    # Register all nodes
    builder.add_node("intake_normalization", intake_normalization)
    builder.add_node("tool_fingerprint", tool_fingerprint)
    builder.add_node("profile_selector", profile_selector)
    builder.add_node("audit_plan_builder", audit_plan_builder)
    builder.add_node(
        "hypothesis_builder",
        hypothesis_builder_llm if effective_llm else hypothesis_builder,
    )
    builder.add_node("investigation_round", investigation_round)
    builder.add_node("gap_analysis", gap_analysis)
    builder.add_node("freeze_evidence_board", freeze_evidence_board)
    builder.add_node("check_execution", check_execution)
    builder.add_node("skeptic_review", skeptic_review)
    builder.add_node("passport_draft", passport_draft)

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
    builder.add_edge("skeptic_review", "passport_draft")
    builder.add_edge("passport_draft", END)

    return builder.compile()
