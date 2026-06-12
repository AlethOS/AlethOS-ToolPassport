#!/usr/bin/env python3
"""Offline reference normalizer and validator for Tool Identity fixtures."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any
from urllib.parse import SplitResult, urlsplit

from validate_audit_catalog import load_json, validate_instance

ROOT = Path(__file__).resolve().parents[1]
TOOL_SCHEMA = load_json(ROOT / "schemas" / "tool.schema.json")
INTAKE_SCHEMA = load_json(ROOT / "schemas" / "tool-identity-intake.schema.json")
RESOLUTION_SCHEMA = load_json(ROOT / "schemas" / "tool-identity-resolution.schema.json")


class ToolIdentityValidationError(ValueError):
    """Raised when Tool Identity fixtures or semantic rules are invalid."""

    def __init__(self, errors: list[str]) -> None:
        self.errors = tuple(errors)
        super().__init__("\n".join(errors))


def _identifier_key(identifier: dict[str, str]) -> str:
    return f"{identifier['namespace']}:{identifier['value']}"


def _safe_port(parts: SplitResult) -> int | None:
    try:
        return parts.port
    except ValueError:
        return -1


def normalize_url(raw_url: str) -> dict[str, str] | None:
    """Normalize one strong URL identifier without network access."""

    if (
        raw_url.strip() != raw_url
        or not raw_url.isascii()
        or any(character.isspace() for character in raw_url)
        or "\\" in raw_url
    ):
        return None

    parts = urlsplit(raw_url)
    if (
        parts.scheme.lower() != "https"
        or not parts.hostname
        or parts.username is not None
        or parts.password is not None
        or parts.query
        or parts.fragment
        or _safe_port(parts) not in (None, 443)
        or "%" in parts.path
        or "//" in parts.path
        or "/./" in parts.path
        or "/../" in parts.path
        or parts.path.endswith("/.")
        or parts.path.endswith("/..")
    ):
        return None

    host = parts.hostname.lower()
    if host.endswith("."):
        return None
    path = parts.path.rstrip("/")

    if host == "github.com":
        segments = [segment for segment in path.split("/") if segment]
        if len(segments) != 2:
            return None
        owner, repository = segments
        if repository.lower().endswith(".git"):
            repository = repository[:-4]
        if not owner or not repository:
            return None
        value = f"{owner.lower()}/{repository.lower()}"
        return {
            "namespace": "github",
            "value": value,
            "canonical_url": f"https://github.com/{value}",
        }

    normalized_path = path if path else ""
    value = f"{host}{normalized_path}"
    return {
        "namespace": "url",
        "value": value,
        "canonical_url": f"https://{value}",
    }


def validate_tool(tool: dict[str, Any], label: str = "tool") -> list[str]:
    errors = validate_instance(tool, TOOL_SCHEMA, label)
    if errors:
        return errors

    identifiers = tool["external_identifiers"]
    keys = [_identifier_key(identifier) for identifier in identifiers]
    if len(keys) != len(set(keys)):
        errors.append(f"{label}: external identifier namespace/value pairs must be unique")
    if tool["tool_id"] not in keys:
        errors.append(f"{label}: tool_id must match one external identifier")
    if tool["canonical_url"] not in {
        identifier["canonical_url"] for identifier in identifiers
    }:
        errors.append(f"{label}: canonical_url must match one external identifier")
    for index, identifier in enumerate(identifiers):
        normalized = normalize_url(identifier["canonical_url"])
        if normalized != identifier:
            errors.append(f"{label}.external_identifiers[{index}]: identifier is not canonical")
    normalized_aliases = [alias.casefold() for alias in tool["aliases"]]
    if len(normalized_aliases) != len(set(normalized_aliases)):
        errors.append(f"{label}: aliases must be unique ignoring case")
    return errors


def validate_contract_schemas() -> list[str]:
    """Ensure Tool Identity tool types remain bound to the committed Profile catalog."""

    profile_types = {
        load_json(path)["profile_id"]
        for path in sorted((ROOT / "profiles").glob("*/*.json"))
    }
    tool_types = set(TOOL_SCHEMA["$defs"]["tool_type"]["enum"])
    intake_types = set(INTAKE_SCHEMA["properties"]["tool_type"]["enum"])
    errors: list[str] = []
    if tool_types != profile_types:
        errors.append(
            "tool.schema.json: tool_type enum must match committed Profile IDs "
            f"(schema={sorted(tool_types)!r}, profiles={sorted(profile_types)!r})"
        )
    if intake_types != profile_types:
        errors.append(
            "tool-identity-intake.schema.json: tool_type enum must match committed Profile IDs "
            f"(schema={sorted(intake_types)!r}, profiles={sorted(profile_types)!r})"
        )
    return errors


def _name_matches(intake_name: str, tool: dict[str, Any]) -> bool:
    normalized_name = intake_name.casefold()
    return normalized_name in {
        tool["name"].casefold(),
        *(alias.casefold() for alias in tool["aliases"]),
    }


def resolve_identity(
    intake: dict[str, Any], existing_tools: list[dict[str, Any]]
) -> dict[str, Any]:
    """Resolve an intake using only canonical strong identifiers and local tools."""

    normalized_by_key: dict[str, dict[str, str]] = {}
    invalid_url = False
    for raw_url in intake["urls"]:
        normalized = normalize_url(raw_url)
        if normalized is None:
            invalid_url = True
        else:
            normalized_by_key[_identifier_key(normalized)] = normalized

    normalized_identifiers = [
        normalized_by_key[key] for key in sorted(normalized_by_key)
    ]
    identifier_owners: dict[str, str] = {}
    for tool in existing_tools:
        for identifier in tool["external_identifiers"]:
            identifier_owners[_identifier_key(identifier)] = tool["tool_id"]

    matched_tool_ids = {
        identifier_owners[key] for key in normalized_by_key if key in identifier_owners
    }
    name_candidates = {
        tool["tool_id"] for tool in existing_tools if _name_matches(intake["name"], tool)
    }
    candidate_tool_ids = sorted(matched_tool_ids | name_candidates)

    def result(
        status: str, tool_id: str | None, reason_codes: list[str]
    ) -> dict[str, Any]:
        return {
            "resolution_version": "0.1.0",
            "status": status,
            "normalized_identifiers": normalized_identifiers,
            "tool_id": tool_id,
            "candidate_tool_ids": candidate_tool_ids,
            "reason_codes": sorted(set(reason_codes)),
        }

    if invalid_url:
        return result("needs_review", None, ["invalid_or_ambiguous_url"])
    if len(matched_tool_ids) > 1:
        return result("needs_review", None, ["conflicting_existing_identifiers"])
    if len(matched_tool_ids) == 1:
        matched_tool_id = next(iter(matched_tool_ids))
        unmatched_keys = set(normalized_by_key) - set(identifier_owners)
        if unmatched_keys:
            return result(
                "needs_review",
                None,
                ["additional_identifier_requires_review"],
            )
        return result("resolved", matched_tool_id, ["existing_identifier_match"])
    if not normalized_identifiers:
        reason = "name_match_only" if name_candidates else "name_only"
        return result("needs_review", None, [reason])
    if len(normalized_identifiers) > 1:
        return result("needs_review", None, ["multiple_strong_identifiers"])
    if name_candidates:
        return result(
            "needs_review",
            None,
            ["possible_fork_or_source_migration"],
        )

    proposed_tool_id = _identifier_key(normalized_identifiers[0])
    return result("create_candidate", proposed_tool_id, ["new_strong_identifier"])


def validate_resolution(
    resolution: dict[str, Any],
    existing_tools: list[dict[str, Any]],
    label: str = "resolution",
) -> list[str]:
    errors = validate_instance(resolution, RESOLUTION_SCHEMA, label)
    if errors:
        return errors

    existing_ids = {tool["tool_id"] for tool in existing_tools}
    identifier_owners = {
        _identifier_key(identifier): tool["tool_id"]
        for tool in existing_tools
        for identifier in tool["external_identifiers"]
    }
    status = resolution["status"]
    tool_id = resolution["tool_id"]
    candidate_ids = resolution["candidate_tool_ids"]
    reason_codes = set(resolution["reason_codes"])
    normalized_identifiers = resolution["normalized_identifiers"]
    normalized_keys = [_identifier_key(identifier) for identifier in normalized_identifiers]
    for index, identifier in enumerate(normalized_identifiers):
        if normalize_url(identifier["canonical_url"]) != identifier:
            errors.append(f"{label}.normalized_identifiers[{index}]: identifier is not canonical")
    if len(normalized_keys) != len(set(normalized_keys)):
        errors.append(f"{label}: normalized identifier namespace/value pairs must be unique")
    unknown_candidates = set(candidate_ids) - existing_ids
    if unknown_candidates:
        errors.append(
            f"{label}: candidate_tool_ids must reference existing Tools "
            f"{sorted(unknown_candidates)!r}"
        )
    if status == "resolved":
        if tool_id not in existing_ids:
            errors.append(f"{label}: resolved tool_id must reference an existing Tool")
        if tool_id not in candidate_ids:
            errors.append(f"{label}: resolved tool_id must be included in candidate_tool_ids")
        if not normalized_keys or any(
            identifier_owners.get(key) != tool_id for key in normalized_keys
        ):
            errors.append(
                f"{label}: resolved identifiers must all belong to the selected Tool"
            )
        if reason_codes != {"existing_identifier_match"}:
            errors.append(f"{label}: resolved must only declare existing_identifier_match")
    elif status == "create_candidate":
        if not isinstance(tool_id, str):
            errors.append(f"{label}: create_candidate must propose a tool_id")
        elif tool_id in existing_ids:
            errors.append(f"{label}: create_candidate tool_id must not already exist")
        if len(normalized_keys) != 1:
            errors.append(f"{label}: create_candidate must contain exactly one strong identifier")
        elif tool_id != normalized_keys[0]:
            errors.append(f"{label}: create_candidate tool_id must match its strong identifier")
        elif normalized_keys[0] in identifier_owners:
            errors.append(f"{label}: create_candidate strong identifier must not be occupied")
        if candidate_ids:
            errors.append(f"{label}: create_candidate must not include existing candidates")
        if reason_codes != {"new_strong_identifier"}:
            errors.append(f"{label}: create_candidate must only declare new_strong_identifier")
    elif status == "needs_review":
        if tool_id is not None:
            errors.append(f"{label}: needs_review must not select or propose a tool_id")
        if reason_codes & {"existing_identifier_match", "new_strong_identifier"}:
            errors.append(f"{label}: needs_review must only declare review reason codes")
    return errors


def validate_fixture(path: Path) -> None:
    fixture = load_json(path)
    expected_keys = {
        "fixture_version",
        "scenario_id",
        "description",
        "existing_tools",
        "intake",
        "expected_resolution",
    }
    errors: list[str] = []
    if not isinstance(fixture, dict):
        raise ToolIdentityValidationError([f"{path}: fixture must be an object"])
    extra = set(fixture) - expected_keys
    missing = expected_keys - set(fixture)
    if extra:
        errors.append(f"{path}: unexpected fixture properties {sorted(extra)!r}")
    if missing:
        errors.append(f"{path}: missing fixture properties {sorted(missing)!r}")
    if errors:
        raise ToolIdentityValidationError(errors)
    if fixture["fixture_version"] != "0.1.0":
        errors.append(f"{path}: fixture_version must be '0.1.0'")
    if not isinstance(fixture["scenario_id"], str) or not fixture["scenario_id"]:
        errors.append(f"{path}: scenario_id must be a non-empty string")
    if not isinstance(fixture["description"], str) or not fixture["description"]:
        errors.append(f"{path}: description must be a non-empty string")
    if not isinstance(fixture["existing_tools"], list):
        errors.append(f"{path}: existing_tools must be an array")
    else:
        tool_ids: set[str] = set()
        identifier_owners: dict[str, str] = {}
        for index, tool in enumerate(fixture["existing_tools"]):
            errors.extend(validate_tool(tool, f"{path}.existing_tools[{index}]"))
            if not isinstance(tool, dict):
                continue
            tool_id = tool.get("tool_id")
            if isinstance(tool_id, str):
                if tool_id in tool_ids:
                    errors.append(f"{path}: duplicate existing tool_id {tool_id!r}")
                tool_ids.add(tool_id)
            identifiers = tool.get("external_identifiers")
            if not isinstance(identifiers, list):
                continue
            for identifier in identifiers:
                if not isinstance(identifier, dict):
                    continue
                namespace = identifier.get("namespace")
                value = identifier.get("value")
                if not isinstance(namespace, str) or not isinstance(value, str):
                    continue
                key = f"{namespace}:{value}"
                owner = identifier_owners.get(key)
                if owner is not None and owner != tool_id:
                    errors.append(
                        f"{path}: external identifier {key!r} is owned by multiple Tools"
                    )
                elif isinstance(tool_id, str):
                    identifier_owners[key] = tool_id
    errors.extend(validate_instance(fixture["intake"], INTAKE_SCHEMA, f"{path}.intake"))
    if isinstance(fixture["existing_tools"], list):
        errors.extend(
            validate_resolution(
                fixture["expected_resolution"],
                fixture["existing_tools"],
                f"{path}.expected_resolution",
            )
        )
    if not errors:
        actual = resolve_identity(fixture["intake"], fixture["existing_tools"])
        if actual != fixture["expected_resolution"]:
            errors.append(
                f"{path}: expected resolution does not match reference resolver\n"
                f"expected={json.dumps(fixture['expected_resolution'], sort_keys=True)}\n"
                f"actual={json.dumps(actual, sort_keys=True)}"
            )
    if errors:
        raise ToolIdentityValidationError(errors)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixtures", nargs="*", type=Path)
    args = parser.parse_args()
    fixture_paths = args.fixtures or sorted(
        (ROOT / "fixtures" / "tool-identity").glob("*/*.json")
    )
    try:
        schema_errors = validate_contract_schemas()
        if schema_errors:
            raise ToolIdentityValidationError(schema_errors)
        for path in fixture_paths:
            validate_fixture(path)
    except (ToolIdentityValidationError, OSError, json.JSONDecodeError, ValueError) as error:
        print(f"FAIL tool identity\n{error}", file=sys.stderr)
        return 1
    print(f"PASS tool identity: {len(fixture_paths)} fixture(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
