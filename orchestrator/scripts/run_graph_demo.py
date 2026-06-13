#!/usr/bin/env python
"""Demonstration script for Stage 3 orchestrator investigation graph.

Run from the repo root:
    PYTHONPATH=src python scripts/run_graph_demo.py

Or via check_orchestrator.sh (which sets up the venv).

Enable GLM-powered gap generation:
    ORCHESTRATOR_USE_LLM=true python scripts/run_graph_demo.py
"""

from __future__ import annotations

import json
import os

from toolpassport_orchestrator import GraphState, build_graph
from toolpassport_orchestrator.fixtures import MOCK_TOOL


def main() -> None:
    use_llm = os.environ.get("ORCHESTRATOR_USE_LLM", "").lower() in ("true", "1", "yes")

    initial = GraphState(
        run_id="00000000-0000-0000-0000-000000000001",
        goal="Audit LangGraph as an agent framework for long-horizon AI workflows",
        audit_directives="Focus on permission isolation and human gate support",
        tool_id=MOCK_TOOL["tool_id"],
        tool_name=MOCK_TOOL["name"],
        tool_type=MOCK_TOOL["tool_type"],
        target_revision=MOCK_TOOL["target_revision"],
    )

    mode_label = "LLM (GLM)" if use_llm else "mock"
    print(f"=== ToolPassport Investigation [{mode_label} mode] ===")
    print(f"tool_id       : {initial.tool_id}")
    print(f"goal          : {initial.goal}")
    print(f"directives    : {initial.audit_directives}")
    print(f"max_rounds    : {initial.research_budget.max_rounds}")
    print()

    graph = build_graph(use_llm=use_llm)
    result = GraphState.model_validate(graph.invoke(initial))

    print(f"phase              : {result.phase}")
    print(f"current_node       : {result.current_node}")
    print(f"research_round     : {result.research_round}")
    print(f"stop_reason        : {result.stop_reason}")
    print(f"evidence_count     : {len(result.evidence_board)}")
    print(
        f"open_gaps          : "
        f"{sum(1 for g in result.open_gaps if not g.resolved)} remaining"
    )
    print(f"review_issues      : {result.review_issues}")
    print()

    print("=== Check Findings ===")
    for f in result.check_findings:
        print(f"  [{f.finding:8s}] {f.check_id}  (evidence: {len(f.evidence_ids)})")

    if result.passport_draft:
        print()
        print("=== Passport Draft (summary) ===")
        draft = dict(result.passport_draft)
        # Trim large arrays for display
        draft.pop("check_findings", None)
        draft.pop("evidence_board", None)
        print(json.dumps(draft, indent=2))


if __name__ == "__main__":
    main()
