from toolpassport_orchestrator import build_graph


def test_mock_graph_reaches_plan_audit() -> None:
    result = build_graph().invoke(
        {
            "run_id": "00000000-0000-0000-0000-000000000001",
            "goal": "Audit a mock AI tool",
            "current_node": "",
            "research_round": 0,
            "evidence_ids": [],
            "artifact_ids": [],
            "errors": [],
            "approval_status": "not_requested",
        }
    )

    assert result["current_node"] == "plan_audit"
