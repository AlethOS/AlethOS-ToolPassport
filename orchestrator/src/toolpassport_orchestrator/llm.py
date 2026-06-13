"""Thin GLM client adapter for the ToolPassport orchestrator.

Uses httpx to call the OpenAI-compatible chat completions endpoint on z.ai.
Reads configuration from environment variables:
  ZAI_API_KEY  — API key (required for live calls)
  ZAI_BASE_URL — base URL (default: https://api.z.ai/api/paas/v4)
  ZAI_MODEL    — model name (default: glm-5.1)

All structured LLM output must be validated via Pydantic before use.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from typing import Any, TypeVar

import httpx
from pydantic import BaseModel

T = TypeVar("T", bound=BaseModel)


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class LLMError(Exception):
    """Raised when the LLM API call fails (network, auth, rate limit, etc.)."""


class LLMValidationError(LLMError):
    """Raised when LLM output fails Pydantic validation."""

    def __init__(self, raw: str, cause: Exception) -> None:
        self.raw = raw
        self.cause = cause
        super().__init__(f"LLM output validation failed: {cause}")


# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class LLMConfig:
    """LLM connection configuration, populated from environment variables."""

    api_key: str = field(default_factory=lambda: os.environ.get("ZAI_API_KEY", ""))
    base_url: str = field(
        default_factory=lambda: os.environ.get(
            "ZAI_BASE_URL", "https://api.z.ai/api/paas/v4"
        )
    )
    model: str = field(
        default_factory=lambda: os.environ.get("ZAI_MODEL", "glm-5.1")
    )
    timeout: float = 120.0

    @property
    def is_configured(self) -> bool:
        return bool(self.api_key)


# ---------------------------------------------------------------------------
# Low-level chat call (OpenAI-compatible)
# ---------------------------------------------------------------------------


def chat(
    config: LLMConfig,
    system_prompt: str,
    user_prompt: str,
    *,
    temperature: float = 0.3,
    max_tokens: int = 8192,
) -> str:
    """Send a single-turn chat completion request and return the content text.

    Raises ``LLMError`` on network failure, non-2xx response, or empty content.
    """
    if not config.is_configured:
        raise LLMError("ZAI_API_KEY is not set; cannot call LLM")

    url = f"{config.base_url.rstrip('/')}/chat/completions"
    headers = {
        "Authorization": f"Bearer {config.api_key}",
        "Content-Type": "application/json",
    }
    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": user_prompt},
    ]
    body: dict[str, Any] = {
        "model": config.model,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
    }

    try:
        with httpx.Client(timeout=config.timeout) as client:
            resp = client.post(url, headers=headers, json=body)
    except httpx.HTTPError as exc:
        raise LLMError(f"HTTP request failed: {exc}") from exc

    if resp.status_code != 200:
        raise LLMError(
            f"LLM API returned {resp.status_code}: {resp.text[:500]}"
        )

    try:
        data = resp.json()
        content: str = data["choices"][0]["message"]["content"]
        if not content:
            raise LLMError("LLM returned empty content (all tokens used for reasoning)")
        return content
    except (KeyError, IndexError, json.JSONDecodeError) as exc:
        raise LLMError(f"Unexpected LLM response structure: {exc}") from exc


# ---------------------------------------------------------------------------
# Structured output with Pydantic validation
# ---------------------------------------------------------------------------


def _strip_markdown_fences(text: str) -> str:
    """Remove markdown code fences if the model wrapped the JSON."""
    cleaned = text.strip()
    if cleaned.startswith("```"):
        lines = cleaned.splitlines()
        lines = [line for line in lines if not line.strip().startswith("```")]
        cleaned = "\n".join(lines).strip()
    return cleaned


def chat_structured(
    config: LLMConfig,
    system_prompt: str,
    user_prompt: str,
    response_model: type[T],
    *,
    temperature: float = 0.3,
) -> T:
    """Call the LLM and validate the response against a Pydantic model.

    Instructs the model to return JSON and parses the output with
    ``response_model``.  Raises ``LLMValidationError`` if the output
    fails validation.  Raises ``LLMError`` if the API call itself fails.
    """
    schema = response_model.model_json_schema()
    json_schema_desc = json.dumps(schema, ensure_ascii=False, indent=2)

    enhanced_system = (
        f"{system_prompt}\n\n"
        "You MUST respond with valid JSON that conforms to this schema:\n"
        f"```json\n{json_schema_desc}\n```\n"
        "Return ONLY the JSON object, no extra text or markdown fences."
    )

    raw = chat(config, enhanced_system, user_prompt, temperature=temperature)
    cleaned = _strip_markdown_fences(raw)

    try:
        return response_model.model_validate_json(cleaned)
    except Exception as exc:
        raise LLMValidationError(raw, exc) from exc


def chat_structured_list(
    config: LLMConfig,
    system_prompt: str,
    user_prompt: str,
    item_model: type[T],
    *,
    temperature: float = 0.3,
) -> list[T]:
    """Call the LLM and validate the response as a JSON array of Pydantic items.

    Raises ``LLMValidationError`` if the output fails validation.
    """
    schema = item_model.model_json_schema()
    json_schema_desc = json.dumps(schema, ensure_ascii=False, indent=2)

    enhanced_system = (
        f"{system_prompt}\n\n"
        "Each element in the JSON array must conform to this schema:\n"
        f"```json\n{json_schema_desc}\n```\n"
        "Return ONLY a JSON array, no extra text or markdown fences."
    )

    raw = chat(config, enhanced_system, user_prompt, temperature=temperature)
    cleaned = _strip_markdown_fences(raw)

    # The model might return {"items": [...]} or just [...]
    try:
        parsed = json.loads(cleaned)
    except json.JSONDecodeError as exc:
        raise LLMValidationError(raw, exc) from exc

    if isinstance(parsed, dict):
        # Try common wrapper keys
        items = parsed.get("items") or parsed.get("gaps") or parsed.get("data")
        if items is None:
            # Maybe the dict itself has array values — look for first list value
            for v in parsed.values():
                if isinstance(v, list):
                    items = v
                    break
        if items is None:
            raise LLMValidationError(raw, ValueError("Cannot find array in response"))
        parsed = items

    if not isinstance(parsed, list):
        raise LLMValidationError(
            raw, TypeError(f"Expected array, got {type(parsed).__name__}")
        )

    try:
        return [item_model.model_validate(item) for item in parsed]
    except Exception as exc:
        raise LLMValidationError(raw, exc) from exc
