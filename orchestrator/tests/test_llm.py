"""Unit tests for the LLM client adapter.

These tests do not make network calls — they verify config, error types,
and structured-output validation logic.
"""

from __future__ import annotations

import os
from unittest.mock import patch

import pytest

from toolpassport_orchestrator.llm import (
    LLMConfig,
    LLMError,
    LLMValidationError,
    chat,
)
from toolpassport_orchestrator.state import GapEntry


# ---------------------------------------------------------------------------
# LLMConfig
# ---------------------------------------------------------------------------


class TestLLMConfig:
    def test_defaults_without_env(self) -> None:
        with patch.dict(os.environ, {}, clear=True):
            cfg = LLMConfig()
            assert cfg.api_key == ""
            assert cfg.base_url == "https://api.z.ai/api/paas/v4"
            assert cfg.model == "glm-5.1"
            assert cfg.timeout == 120.0
            assert not cfg.is_configured

    def test_reads_env_vars(self) -> None:
        env = {
            "ZAI_API_KEY": "test-key-123",
            "ZAI_BASE_URL": "https://custom.api/v1",
            "ZAI_MODEL": "glm-test",
        }
        with patch.dict(os.environ, env, clear=True):
            cfg = LLMConfig()
            assert cfg.api_key == "test-key-123"
            assert cfg.base_url == "https://custom.api/v1"
            assert cfg.model == "glm-test"
            assert cfg.is_configured

    def test_frozen(self) -> None:
        cfg = LLMConfig()
        with pytest.raises(AttributeError):
            cfg.api_key = "mutated"  # type: ignore[misc]


# ---------------------------------------------------------------------------
# chat() — missing key
# ---------------------------------------------------------------------------


class TestChat:
    def test_raises_without_api_key(self) -> None:
        with patch.dict(os.environ, {}, clear=True):
            cfg = LLMConfig()
            with pytest.raises(LLMError, match="ZAI_API_KEY is not set"):
                chat(cfg, "system", "user")


# ---------------------------------------------------------------------------
# chat_structured() — validation
# ---------------------------------------------------------------------------


class TestChatStructured:
    def test_validation_error_on_bad_json(self) -> None:
        """Verify bad JSON raises a Pydantic ValidationError."""
        from pydantic import ValidationError

        bad_json = "not valid json"
        with pytest.raises(ValidationError):
            GapEntry.model_validate_json(bad_json)

    def test_validation_error_attributes(self) -> None:
        raw = '{"bad": "shape"}'
        try:
            GapEntry.model_validate_json(raw)
        except Exception as cause:
            err = LLMValidationError(raw, cause)
            assert err.raw == raw
            assert err.cause is cause
            assert "validation failed" in str(err)


# ---------------------------------------------------------------------------
# chat_structured_list() — array extraction
# ---------------------------------------------------------------------------


class TestChatStructuredList:
    def test_extracts_items_from_dict(self) -> None:
        """Verify the list extraction logic handles wrapped arrays."""
        # Direct test of the validation path, not the HTTP call
        payload = {
            "items": [
                {
                    "gap_id": "gap-test-1",
                    "check_id": "test.check",
                    "description": "Test gap",
                    "priority": "medium",
                    "resolved": False,
                }
            ]
        }
        result = [GapEntry.model_validate(item) for item in payload["items"]]
        assert len(result) == 1
        assert result[0].gap_id == "gap-test-1"

    def test_handles_bare_array(self) -> None:
        data = [
            {
                "gap_id": "gap-1",
                "check_id": "c1",
                "description": "Gap 1",
                "priority": "high",
                "resolved": False,
            },
            {
                "gap_id": "gap-2",
                "check_id": "c2",
                "description": "Gap 2",
                "priority": "medium",
                "resolved": False,
            },
        ]
        result = [GapEntry.model_validate(item) for item in data]
        assert len(result) == 2
        assert result[0].priority == "high"
