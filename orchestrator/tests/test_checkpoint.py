"""Tests for LangGraph checkpoint persistence and process-restart recovery."""

from pathlib import Path

from langgraph.checkpoint.memory import MemorySaver

from toolpassport_orchestrator import GraphState, build_graph
from toolpassport_orchestrator.checkpoint import (
    checkpoint_config,
    has_checkpoint,
    sqlite_checkpointer,
)
from toolpassport_orchestrator.fixtures import MOCK_TOOL
from toolpassport_orchestrator.state import ResearchBudget


def _initial_budget() -> ResearchBudget:
    return ResearchBudget(max_rounds=3, max_sources=30, sources_used=0)


def test_graph_runs_with_checkpointer() -> None:
    """The graph should complete normally when a MemorySaver is attached."""
    checkpointer = MemorySaver()
    graph = build_graph(checkpointer=checkpointer)

    initial = GraphState(
        run_id="checkpoint-test-run",
        goal="Verify checkpoint integration",
        tool_id=MOCK_TOOL["tool_id"],
        tool_name=MOCK_TOOL["name"],
        tool_type=MOCK_TOOL["tool_type"],
        target_revision=MOCK_TOOL["target_revision"],
        research_budget=_initial_budget(),
    )

    config = {"configurable": {"thread_id": initial.run_id}}
    result = GraphState.model_validate(graph.invoke(initial, config))
    assert result.phase == "done"
    assert len(result.evidence_board) > 0
    assert len(result.check_findings) > 0


def test_checkpoint_saves_and_resumes() -> None:
    """State should persist across invocations with the same thread_id."""
    checkpointer = MemorySaver()
    graph = build_graph(checkpointer=checkpointer)

    initial = GraphState(
        run_id="checkpoint-resume-run",
        goal="Verify checkpoint resume",
        tool_id=MOCK_TOOL["tool_id"],
        tool_name=MOCK_TOOL["name"],
        tool_type=MOCK_TOOL["tool_type"],
        target_revision=MOCK_TOOL["target_revision"],
        research_budget=_initial_budget(),
    )

    config = {"configurable": {"thread_id": initial.run_id}}

    # First invocation: runs to completion.
    result1 = GraphState.model_validate(graph.invoke(initial, config))
    assert result1.phase == "done"
    evidence_count_1 = len(result1.evidence_board)
    assert evidence_count_1 > 0

    # Second invocation with None: resumes from last checkpoint (already done).
    result2 = GraphState.model_validate(graph.invoke(None, config))
    # Should still be done with the same state.
    assert result2.phase == "done"
    assert len(result2.evidence_board) == evidence_count_1


def test_sqlite_checkpoint_resumes_across_graph_instances(tmp_path: Path) -> None:
    """A fresh process-equivalent graph should resume from persisted SQLite state."""
    database_path = str(tmp_path / "orchestrator-checkpoints.sqlite")
    initial = GraphState(
        run_id="persistent-checkpoint-run",
        goal="Verify persistent checkpoint recovery",
        tool_id=MOCK_TOOL["tool_id"],
        tool_name=MOCK_TOOL["name"],
        tool_type=MOCK_TOOL["tool_type"],
        target_revision=MOCK_TOOL["target_revision"],
        research_budget=_initial_budget(),
    )
    config = checkpoint_config(initial.run_id)

    with sqlite_checkpointer(database_path) as first_checkpointer:
        first_graph = build_graph(checkpointer=first_checkpointer)
        assert not has_checkpoint(first_checkpointer, config)
        interrupted_result = GraphState.model_validate(
            first_graph.invoke(initial, config, interrupt_after=["audit_plan_builder"])
        )
        assert has_checkpoint(first_checkpointer, config)
        assert interrupted_result.current_node == "audit_plan_builder"
        assert interrupted_result.phase != "done"
        assert not interrupted_result.evidence_board

    with sqlite_checkpointer(database_path) as restarted_checkpointer:
        restarted_graph = build_graph(checkpointer=restarted_checkpointer)
        assert has_checkpoint(restarted_checkpointer, config)
        resumed_result = GraphState.model_validate(restarted_graph.invoke(None, config))

    assert resumed_result.phase == "done"
    assert resumed_result.evidence_board
    assert resumed_result.check_findings
