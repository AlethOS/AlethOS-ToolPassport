"""Tests for controlled live web research."""

from __future__ import annotations

import socket
from unittest.mock import patch

import httpx
import pytest

from toolpassport_orchestrator.nodes import _fetch_real_sources, investigation_round
from toolpassport_orchestrator.research import Researcher, SourcePage, validate_url
from toolpassport_orchestrator.state import GraphState, ResearchBudget


def test_validate_url_rejects_non_https_and_private_hosts() -> None:
    with pytest.raises(ValueError, match="Only HTTPS"):
        validate_url("http://example.com")
    with pytest.raises(ValueError, match="private/loopback"):
        validate_url("https://127.0.0.1/private")


def test_fetch_revalidates_and_rejects_cross_host_redirect() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(302, headers={"location": "https://127.0.0.1/private"})

    researcher = Researcher(transport=httpx.MockTransport(handler))
    with patch("socket.getaddrinfo", return_value=[]):
        with pytest.raises(ValueError, match="private/loopback"):
            researcher.fetch("https://example.com/start")
    researcher.close()


def test_live_source_collection_uses_run_url_without_claiming_support() -> None:
    class StubResearcher:
        def fetch_github_repo(self, owner: str, repo: str) -> object:
            assert (owner, repo) == ("example", "actual-tool")
            return type(
                "Result",
                (),
                {
                    "pages": [
                        SourcePage(
                            url="https://github.com/example/actual-tool",
                            status_code=200,
                            content_type="text/plain",
                            text="Actual project documentation",
                            size_bytes=28,
                        )
                    ],
                    "errors": [],
                },
            )()

        def extract_summary(self, page: SourcePage, max_chars: int = 1_500) -> str:
            return page.text[:max_chars]

    state = GraphState(
        run_id="00000000-0000-0000-0000-000000000010",
        goal="Audit the actual tool",
        research_mode="live",
        tool_id="github:example/actual-tool",
        tool_name="actual-tool",
        tool_type="generic",
        canonical_url="https://github.com/example/actual-tool",
    )

    entries = _fetch_real_sources(StubResearcher(), state, 0, "2026-06-14T00:00:00Z")  # type: ignore[arg-type]
    assert len(entries) == 1
    assert entries[0].source_url == state.canonical_url
    assert entries[0].supports == []
    assert entries[0].contradicts == []


def test_live_round_does_not_fall_back_to_mock_when_research_fails() -> None:
    state = GraphState(
        run_id="00000000-0000-0000-0000-000000000011",
        goal="Audit unavailable live source",
        research_mode="live",
        tool_id="github:example/unavailable",
        tool_name="unavailable",
        tool_type="generic",
        canonical_url="https://github.com/example/unavailable",
        profile_id="generic",
        profile_version="0.2.0",
        research_budget=ResearchBudget(max_rounds=1, max_sources=2),
    )

    with patch(
        "toolpassport_orchestrator.nodes._fetch_real_sources",
        side_effect=socket.gaierror("unavailable"),
    ):
        updates = investigation_round(state)

    assert updates["evidence_board"] == []
    assert updates["research_budget"].sources_used == 0
    assert any(error.startswith("live_research_failed:") for error in updates["errors"])
