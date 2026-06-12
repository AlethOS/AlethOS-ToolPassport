from typing import Literal

from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from typing_extensions import TypedDict


class GraphState(TypedDict):
    run_id: str
    goal: str
    current_node: str
    research_round: int
    evidence_ids: list[str]
    artifact_ids: list[str]
    errors: list[str]
    approval_status: Literal["not_requested", "waiting", "approved", "rejected"]


def clarify_goal(state: GraphState) -> dict[str, str]:
    if not state["goal"].strip():
        raise ValueError("goal must not be empty")
    return {"current_node": "clarify_goal"}


def plan_audit(state: GraphState) -> dict[str, str]:
    del state
    return {"current_node": "plan_audit"}


def build_graph() -> CompiledStateGraph[GraphState, None, GraphState, GraphState]:
    builder: StateGraph[GraphState, None, GraphState, GraphState] = StateGraph(GraphState)
    builder.add_node("clarify_goal", clarify_goal, input_schema=GraphState)
    builder.add_node("plan_audit", plan_audit, input_schema=GraphState)
    builder.add_edge(START, "clarify_goal")
    builder.add_edge("clarify_goal", "plan_audit")
    builder.add_edge("plan_audit", END)
    return builder.compile()
