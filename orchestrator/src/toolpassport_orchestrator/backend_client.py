"""HTTP client for the Rust Trust Core backend API.

Uses ``httpx`` to call the backend endpoints. Every method wraps the call
in try/except and returns ``None`` on failure so graph nodes can fall back
to mock data without crashing the investigation.
"""

from __future__ import annotations

import logging
import os
from dataclasses import dataclass
from typing import Any, cast

import httpx

logger = logging.getLogger(__name__)

DEFAULT_BACKEND_URL = os.environ.get("BACKEND_URL", "http://127.0.0.1:8080")


@dataclass
class BackendClient:
    """Synchronous HTTP client for the Rust Trust Core API.

    Not stored in GraphState — use a module-level singleton or pass via
    closure to avoid breaking LangGraph checkpoint serialization.
    """

    base_url: str = DEFAULT_BACKEND_URL
    timeout: float = 30.0

    def __post_init__(self) -> None:
        self._client = httpx.Client(
            base_url=self.base_url,
            timeout=self.timeout,
        )

    # ── Helpers ─────────────────────────────────────────────────

    def _post(self, path: str, json_payload: dict[str, Any] | None = None) -> dict[str, Any]:
        response = self._client.post(path, json=json_payload)
        response.raise_for_status()
        return cast(dict[str, Any], response.json())

    def _get(self, path: str) -> dict[str, Any]:
        response = self._client.get(path)
        response.raise_for_status()
        return cast(dict[str, Any], response.json())

    # ── Run Lifecycle ───────────────────────────────────────────

    def create_run(self, goal: str, tool_id: str) -> dict[str, Any] | None:
        """Create a new audit run in the backend. Returns the Run dict or None."""
        try:
            return self._post(
                "/api/runs",
                {"goal": goal, "tool_id": tool_id},
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend create_run failed: %s", exc)
            return None

    def get_run_details(self, run_id: str) -> dict[str, Any] | None:
        """Load the Rust-owned Run snapshot and append-only events."""
        try:
            return self._get(f"/api/runs/{run_id}")
        except httpx.HTTPError as exc:
            logger.warning("Backend get_run_details failed: %s", exc)
            return None

    def append_event(
        self,
        run_id: str,
        node_id: str,
        event_type: str,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any] | None:
        """Append a run event. Returns the created RunEvent dict or None."""
        try:
            return self._post(
                f"/api/runs/{run_id}/events",
                {
                    "node_id": node_id,
                    "event_type": event_type,
                    "payload": payload or {},
                },
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend append_event (%s) failed: %s", event_type, exc)
            return None

    # ── Evidence / Artifacts ─────────────────────────────────────

    def create_evidence(
        self,
        run_id: str,
        evidence: dict[str, Any],
    ) -> dict[str, Any] | None:
        """Create an Evidence entry. Returns the Evidence dict or None."""
        try:
            return self._post(
                f"/api/runs/{run_id}/evidence",
                evidence,
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend create_evidence failed: %s", exc)
            return None

    def upload_artifact(
        self,
        run_id: str,
        filename: str,
        content: bytes,
        content_type: str = "application/octet-stream",
    ) -> dict[str, Any] | None:
        """Upload an Artifact file via multipart. Returns the Artifact dict or None."""
        try:
            response = self._client.post(
                f"/api/runs/{run_id}/artifacts",
                files={"file": (filename, content, content_type)},
            )
            response.raise_for_status()
            return cast(dict[str, Any], response.json())
        except httpx.HTTPError as exc:
            logger.warning("Backend upload_artifact failed: %s", exc)
            return None

    # ── Freeze / Scoring / Passport ──────────────────────────────

    def freeze_evidence_board(
        self,
        run_id: str,
        request: dict[str, Any],
    ) -> dict[str, Any] | None:
        """Submit an evidence board freeze proposal. Returns the freeze result or None."""
        try:
            return self._post(
                f"/api/runs/{run_id}/evidence-board/freeze",
                request,
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend freeze_evidence_board failed: %s", exc)
            return None

    def submit_check_results(
        self,
        run_id: str,
        submission: dict[str, Any],
    ) -> dict[str, Any] | None:
        """Submit check results for deterministic scoring. Returns CheckResults or None."""
        try:
            return self._post(
                f"/api/runs/{run_id}/check-results",
                submission,
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend submit_check_results failed: %s", exc)
            return None

    def freeze_passport(
        self,
        run_id: str,
        request: dict[str, Any],
    ) -> dict[str, Any] | None:
        """Submit a passport freeze proposal. Returns PassportFreezeResult or None."""
        try:
            return self._post(
                f"/api/runs/{run_id}/passport/freeze",
                request,
            )
        except httpx.HTTPError as exc:
            logger.warning("Backend freeze_passport failed: %s", exc)
            return None

    def close(self) -> None:
        """Release the underlying HTTP client."""
        self._client.close()


# Module-level singleton for graph nodes to use without carrying it in
# GraphState. Set via ``set_backend_client()`` before invoking the graph.
_backend_client: BackendClient | None = None


def set_backend_client(client: BackendClient | None) -> None:
    global _backend_client
    _backend_client = client


def get_backend_client() -> BackendClient | None:
    return _backend_client
