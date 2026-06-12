from toolpassport_orchestrator import build_graph


def main() -> None:
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
    print(result)


if __name__ == "__main__":
    main()
