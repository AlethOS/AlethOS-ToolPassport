"""Typed Graph State for the ToolPassport investigation orchestrator."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, Field


class ResearchBudget(BaseModel):
    max_rounds: int = 3
    max_sources: int = 30
    sources_used: int = 0


class EvidenceEntry(BaseModel):
    evidence_schema_version: str = "0.2.0"
    evidence_id: str
    source_type: str
    source_url: str
    source_revision: str | None = None
    title: str
    excerpt: str
    retrieved_at: str = ""
    snapshot_artifact_id: str | None = None
    supports: list[str] = Field(default_factory=list)
    contradicts: list[str] = Field(default_factory=list)

    @classmethod
    def from_backend(cls, data: dict[str, Any]) -> EvidenceEntry:
        """Build an EvidenceEntry from a backend Evidence response dict."""
        return cls(
            evidence_schema_version=data.get("evidence_schema_version", "0.2.0"),
            evidence_id=data["evidence_id"],
            source_type=data["source_type"],
            source_url=data.get("source_url", ""),
            source_revision=data.get("source_revision"),
            title=data["title"],
            excerpt=data.get("excerpt", ""),
            retrieved_at=data.get("retrieved_at", ""),
            snapshot_artifact_id=data.get("snapshot_artifact_id"),
            supports=data.get("supports", []),
            contradicts=data.get("contradicts", []),
        )


class GapEntry(BaseModel):
    gap_id: str
    check_id: str
    description: str
    priority: Literal["high", "medium", "low"] = "medium"
    resolved: bool = False


class CheckFinding(BaseModel):
    check_id: str
    finding: Literal["pass", "partial", "fail", "unknown"]
    rationale: str
    evidence_ids: list[str] = Field(default_factory=list)


class FrozenBoardRef(BaseModel):
    """Reference to a frozen evidence board version in the backend."""
    version: int
    frozen_at: str = ""


class CheckResultsRef(BaseModel):
    """Reference to persisted check results in the backend."""
    check_results_id: str
    evidence_board_version: int
    total_score: int = 0
    rating: str = ""


class GraphState(BaseModel):
    run_id: str
    goal: str
    audit_directives: str | None = None
    research_mode: Literal["mock", "live"] = "mock"
    # Tool identity (populated by intake / fingerprint)
    tool_id: str | None = None
    tool_name: str | None = None
    tool_type: str | None = None
    canonical_url: str | None = None
    target_revision: str | None = None
    # Standard & Profile (populated by profile_selector)
    standard_version: str = "0.2.0"
    profile_id: str | None = None
    profile_version: str | None = None
    # Orchestration state
    phase: Literal["intake", "investigation", "evaluation", "done"] = "intake"
    current_node: str = ""
    research_round: int = 0
    research_budget: ResearchBudget = Field(default_factory=ResearchBudget)
    # Evidence workspace
    evidence_board: list[EvidenceEntry] = Field(default_factory=list)
    open_gaps: list[GapEntry] = Field(default_factory=list)
    check_findings: list[CheckFinding] = Field(default_factory=list)
    review_issues: list[str] = Field(default_factory=list)
    # Control
    errors: list[str] = Field(default_factory=list)
    approval_status: Literal[
        "not_requested", "waiting", "approved", "rejected"
    ] = "not_requested"
    stop_reason: str | None = None
    passport_draft: dict[str, Any] | None = None
    # Backend freeze references
    frozen_board: FrozenBoardRef | None = None
    check_results_ref: CheckResultsRef | None = None
    passport_sequence: int | None = None
